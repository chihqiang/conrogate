//! WebSocket 协议升级处理 + 双向字节流转发。

use bytes::Bytes;
use http::{Method, Request, Response, StatusCode};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
/// 检查是否为 WebSocket 升级请求
pub fn is_upgrade_request(req: &Request<Bytes>) -> bool {
    if req.method() != Method::GET {
        return false;
    }

    let headers = req.headers();
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
pub fn build_upgrade_response(req: &Request<Bytes>) -> Response<Bytes> {
    let key = req
        .headers()
        .get("sec-websocket-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let mut hasher = Sha1::new();
    hasher.update(key.as_bytes());
    hasher.update(b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
    let accept = base64_encode(&hasher.finalize());

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
/// 流程：connect upstream → send upgrade request → read 101 response → bidirectional copy
pub async fn forward_websocket<C>(
    upstream_addr: &str,
    client: C,
    upgrade_req: Request<Bytes>,
    timeout: Duration,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    C: tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + Unpin,
{
    // 1. 连接上游（带超时）
    let upstream = tokio::time::timeout(timeout, TcpStream::connect(upstream_addr))
        .await
        .map_err(|_| "upstream connect timeout")??;

    // 2. 将升级请求发送到上游
    let (mut client_r, mut client_w) = tokio::io::split(client);
    let (mut upstream_r, mut upstream_w) = upstream.into_split();

    // 序列化 HTTP 升级请求并发送到上游
    let req_bytes = serialize_request(&upgrade_req);
    upstream_w.write_all(&req_bytes).await?;
    upstream_w.flush().await?;

    // 3. 读取上游的 101 响应并转发给客户端
    // 读取直到 \r\n\r\n
    let mut response_buf = Vec::with_capacity(1024);
    loop {
        let mut buf = [0u8; 256];
        let n = upstream_r.read(&mut buf).await?;
        if n == 0 {
            return Err("upstream closed before sending 101".into());
        }
        response_buf.extend_from_slice(&buf[..n]);
        client_w.write_all(&buf[..n]).await?;
        client_w.flush().await?;

        // 检查是否已读完 HTTP 头部
        if response_buf.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
        if response_buf.len() > 8192 {
            return Err("upgrade response too large".into());
        }
    }

    // 4. 双向字节流透传（带背压处理）
    let c2s = async {
        let mut buf = [0u8; 8192];
        loop {
            let n = client_r.read(&mut buf).await?;
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
        let mut buf = [0u8; 8192];
        loop {
            let n = upstream_r.read(&mut buf).await?;
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

// ── SHA-1 简易实现 ──

struct Sha1 {
    h0: u32,
    h1: u32,
    h2: u32,
    h3: u32,
    h4: u32,
    msg_len: u64,
    buffer: Vec<u8>,
}

impl Sha1 {
    fn new() -> Self {
        Self {
            h0: 0x67452301,
            h1: 0xEFCDAB89,
            h2: 0x98BADCFE,
            h3: 0x10325476,
            h4: 0xC3D2E1F0,
            msg_len: 0,
            buffer: Vec::new(),
        }
    }

    fn update(&mut self, data: &[u8]) {
        self.msg_len += data.len() as u64;
        self.buffer.extend_from_slice(data);
        while self.buffer.len() >= 64 {
            let block: [u8; 64] = self.buffer[..64].try_into().unwrap();
            self.process_block(&block);
            self.buffer.drain(..64);
        }
    }

    fn finalize(mut self) -> [u8; 20] {
        let bit_len = self.msg_len * 8;
        self.buffer.push(0x80);
        while self.buffer.len() % 64 != 56 {
            self.buffer.push(0);
        }
        self.buffer.extend_from_slice(&bit_len.to_be_bytes());
        while self.buffer.len() >= 64 {
            let block: [u8; 64] = self.buffer[..64].try_into().unwrap();
            self.process_block(&block);
            self.buffer.drain(..64);
        }
        let mut result = [0u8; 20];
        result[..4].copy_from_slice(&self.h0.to_be_bytes());
        result[4..8].copy_from_slice(&self.h1.to_be_bytes());
        result[8..12].copy_from_slice(&self.h2.to_be_bytes());
        result[12..16].copy_from_slice(&self.h3.to_be_bytes());
        result[16..20].copy_from_slice(&self.h4.to_be_bytes());
        result
    }

    fn process_block(&mut self, block: &[u8; 64]) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }

        let mut a = self.h0;
        let mut b = self.h1;
        let mut c = self.h2;
        let mut d = self.h3;
        let mut e = self.h4;

        for i in 0..80 {
            let (f, k) = match i {
                0..=19 => ((b & c) | (!b & d), 0x5A827999u32),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(w[i]);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }

        self.h0 = self.h0.wrapping_add(a);
        self.h1 = self.h1.wrapping_add(b);
        self.h2 = self.h2.wrapping_add(c);
        self.h3 = self.h3.wrapping_add(d);
        self.h4 = self.h4.wrapping_add(e);
    }
}

fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    let mut i = 0;
    while i < data.len() {
        let b0 = data[i];
        let b1 = if i + 1 < data.len() { data[i + 1] } else { 0 };
        let b2 = if i + 2 < data.len() { data[i + 2] } else { 0 };

        result.push(TABLE[(b0 >> 2) as usize] as char);
        result.push(TABLE[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        if i + 1 < data.len() {
            result.push(TABLE[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            result.push('=');
        }
        if i + 2 < data.len() {
            result.push(TABLE[(b2 & 0x3f) as usize] as char);
        } else {
            result.push('=');
        }
        i += 3;
    }
    result
}
