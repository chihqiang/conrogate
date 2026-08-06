//! WebSocket 协议升级处理 + 双向字节流转发。

use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use bytes::Bytes;
use http::{HeaderMap, Method, Request, Response, StatusCode};
use sha1::{Digest, Sha1};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// 检查是否为 WebSocket 升级请求（方法 + 头，无请求体依赖）
pub fn is_upgrade_request(method: &Method, headers: &HeaderMap) -> bool {
    if method != Method::GET {
        return false;
    }

    let upgrade = headers
        .get("upgrade")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.eq_ignore_ascii_case("websocket"))
        .unwrap_or(false);

    let connection = headers
        .get("connection")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_ascii_lowercase().contains("upgrade"))
        .unwrap_or(false);

    upgrade && connection
}

/// 构造 WebSocket 握手响应（101 Switching Protocols）
pub fn build_upgrade_response(headers: &HeaderMap) -> Response<Bytes> {
    let key = headers
        .get("sec-websocket-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let mut hasher = Sha1::new();
    hasher.update(key.as_bytes());
    hasher.update(b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
    let accept = BASE64_STANDARD.encode(hasher.finalize());

    Response::builder()
        .status(StatusCode::SWITCHING_PROTOCOLS)
        .header("Upgrade", "websocket")
        .header("Connection", "Upgrade")
        .header("Sec-WebSocket-Accept", accept)
        .body(Bytes::new())
        .unwrap()
}

/// WebSocket 双向转发：连接上游 + 透传字节流
///
/// 流程：connect upstream → send upgrade request → validate 101 → bidirectional copy
/// `connect_timeout` 仅约束上游建连；`idle_timeout` 约束双向数据流的空闲超时
/// （任一方向长时间无数据则关闭隧道）。`buffer_size` 为双向透传缓冲上限（0 时使用默认 8192）。
pub async fn forward_websocket<C>(
    upstream_addr: &str,
    client: C,
    upgrade_req: Request<Bytes>,
    connect_timeout: Duration,
    idle_timeout: Duration,
    buffer_size: usize,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    C: tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + Unpin,
{
    // 1. 连接上游（带超时 + DNS 缓存）
    let addrs = crate::protocol::dns::global_resolver().resolve(upstream_addr).await?;
    let upstream = tokio::time::timeout(connect_timeout, TcpStream::connect(&addrs[..]))
        .await
        .map_err(|_| "upstream connect timeout")??;

    // 2. 将升级请求发送到上游
    let (mut client_r, mut client_w) = tokio::io::split(client);
    let (mut upstream_r, mut upstream_w) = upstream.into_split();

    // 序列化 HTTP 升级请求并发送到上游
    let req_bytes = serialize_request(&upgrade_req);
    upstream_w.write_all(&req_bytes).await?;
    upstream_w.flush().await?;

    // 3. 读取上游握手响应并校验（不转发给客户端：网关已在 HTTP 层向客户端返回 101，
    //    此处仅确认上游接受升级，避免客户端收到重复的 101 导致 WebSocket 解析失败）
    let mut response_buf = Vec::with_capacity(1024);
    loop {
        let mut buf = [0u8; 256];
        let n = upstream_r.read(&mut buf).await?;
        if n == 0 {
            return Err("upstream closed before accepting upgrade".into());
        }
        response_buf.extend_from_slice(&buf[..n]);

        // 检查是否已读完 HTTP 头部
        if response_buf.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
        if response_buf.len() > 8192 {
            return Err("upgrade response too large".into());
        }
    }
    // 校验状态行：101（或 2xx）表示上游接受 WebSocket 升级
    let status_line = response_buf
        .split(|&b| b == b'\n')
        .next()
        .unwrap_or_default();
    if !(status_line.starts_with(b"HTTP/1.1 101")
        || status_line.starts_with(b"HTTP/1.1 2")
        || status_line.starts_with(b"HTTP/2 101")
        || status_line.starts_with(b"HTTP/2 2"))
    {
        return Err("upstream rejected websocket upgrade".into());
    }
    // 保留头部之后已到达的首帧数据（上游可能在握手响应后立即推送帧）
    let header_end = response_buf
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|p| p + 4)
        .unwrap_or(response_buf.len());
    let early_frames = response_buf[header_end..].to_vec();

    // 4. 双向字节流透传（带背压处理 + 空闲超时）
    let buf_size = buffer_size.max(1);
    let c2s = async {
        let mut buf = vec![0u8; buf_size];
        loop {
            let n = tokio::time::timeout(idle_timeout, client_r.read(&mut buf))
                .await
                .map_err(|_| "client idle timeout")??;
            if n == 0 {
                break;
            }
            upstream_w.write_all(&buf[..n]).await?;
            upstream_w.flush().await?;
        }
        let _ = upstream_w.shutdown().await;
        Result::<_, Box<dyn std::error::Error + Send + Sync>>::Ok(())
    };

    let s2c = async {
        // 先写出握手响应后已到达的首帧数据
        if !early_frames.is_empty() {
            client_w.write_all(&early_frames).await?;
            client_w.flush().await?;
        }
        let mut buf = vec![0u8; buf_size];
        loop {
            let n = tokio::time::timeout(idle_timeout, upstream_r.read(&mut buf))
                .await
                .map_err(|_| "upstream idle timeout")??;
            if n == 0 {
                break;
            }
            client_w.write_all(&buf[..n]).await?;
            client_w.flush().await?;
        }
        let _ = client_w.shutdown().await;
        Result::<_, Box<dyn std::error::Error + Send + Sync>>::Ok(())
    };

    // 双向并发，任一方向结束即结束
    tokio::try_join!(c2s, s2c)?;

    Ok(())
}

/// 序列化 HTTP 请求为原始字节
fn serialize_request(req: &Request<Bytes>) -> Vec<u8> {
    let mut buf = Vec::new();
    let method = req.method().as_str();
    let uri = req.uri();
    let path = uri.path_and_query().map(|p| p.as_str()).unwrap_or("/");
    buf.extend_from_slice(format!("{} {} HTTP/1.1\r\n", method, path).as_bytes());

    for (name, value) in req.headers() {
        buf.extend_from_slice(name.as_str().as_bytes());
        buf.extend_from_slice(b": ");
        buf.extend_from_slice(value.as_bytes());
        buf.extend_from_slice(b"\r\n");
    }

    buf.extend_from_slice(b"\r\n");
    buf.extend_from_slice(req.body());
    buf
}
