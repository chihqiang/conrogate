//! 代理转发：HTTP 请求转发 + 响应回传 + TCP 双向转发。

use bytes::Bytes;
use conrogate_contract::dto::UpstreamNodeDto;
use conrogate_contract::ConrogateError;
use http::Request;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper_rustls::HttpsConnector;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use std::time::Duration;
use tokio::net::TcpStream;

/// 统一请求体类型：BoxBody 兼容缓冲模式（Full<Bytes>）和流式模式（Incoming）
pub type ReqBody = http_body_util::combinators::BoxBody<Bytes, Box<dyn std::error::Error + Send + Sync>>;

/// 出站 HTTP 客户端：支持 http:// 与 https://（TLS）上游
pub type HttpClient = Client<HttpsConnector<HttpConnector>, ReqBody>;

/// 代理转发结果
pub struct ProxyResult {
    pub status: http::StatusCode,
    pub headers: http::HeaderMap,
    pub body: Bytes,
}

/// 转发 HTTP 请求到上游节点（缓冲模式：body 已在内存中）
pub async fn forward_http(
    client: &HttpClient,
    node: &UpstreamNodeDto,
    req: Request<ReqBody>,
    timeout: Duration,
) -> Result<ProxyResult, ConrogateError> {
    forward_internal(client, node, req, timeout).await
}

/// 转发 HTTP 请求到上游节点（流式模式：body 以 BoxBody 包装 Incoming，不提前 collect）
pub async fn forward_http_stream(
    client: &HttpClient,
    node: &UpstreamNodeDto,
    req: Request<ReqBody>,
    timeout: Duration,
) -> Result<ProxyResult, ConrogateError> {
    // 流式与缓冲走同一条 client.request() 路径
    // 区别在 body 类型：Incoming 会按帧流式发送，不提前载入内存
    forward_internal(client, node, req, timeout).await
}

/// 内部转发逻辑
async fn forward_internal(
    client: &HttpClient,
    node: &UpstreamNodeDto,
    req: Request<ReqBody>,
    timeout: Duration,
) -> Result<ProxyResult, ConrogateError> {
    // 地址支持显式 scheme（https://host:port）；缺省按 http
    let addr = if node.address.contains("://") {
        node.address.clone()
    } else {
        format!("http://{}", node.address)
    };

    let (method, uri, headers, body) = (
        req.method().clone(),
        req.uri().clone(),
        req.headers().clone(),
        req.into_body(),
    );

    let path_and_query = uri
        .path_and_query()
        .map(|p| p.as_str())
        .unwrap_or("/");
    let upstream_uri: http::Uri = format!("{}{}", addr, path_and_query)
        .parse()
        .map_err(|e| ConrogateError::UpstreamConnectFailed(format!("uri parse: {e}")))?;

    let mut upstream_req = Request::builder()
        .method(method)
        .uri(upstream_uri)
        .body(body)
        .map_err(|e| ConrogateError::UpstreamConnectFailed(format!("request build: {e}")))?;

    *upstream_req.headers_mut() = headers;

    // 发送请求（带超时）
    let response = tokio::time::timeout(timeout, client.request(upstream_req))
        .await
        .map_err(|_| ConrogateError::UpstreamTimeout)?
        .map_err(|e| ConrogateError::UpstreamConnectFailed(e.to_string()))?;

    let (parts, body) = response.into_parts();
    let body_bytes = body
        .collect()
        .await
        .map_err(|e| ConrogateError::UpstreamBadResponse(e.to_string()))?
        .to_bytes();

    Ok(ProxyResult {
        status: parts.status,
        headers: parts.headers,
        body: body_bytes,
    })
}

/// 将 Bytes 包装为 ReqBody（缓冲模式）
pub fn body_from_bytes(bytes: Bytes) -> ReqBody {
    use http_body_util::combinators::BoxBody;
    // Full<Bytes> 的 Error = Infallible，需要 map_err 统一为 BoxError
    BoxBody::new(Full::new(bytes).map_err(|e| match e {}))
}

/// 将 Incoming 包装为 ReqBody（流式模式）
pub fn body_from_incoming(incoming: Incoming) -> ReqBody {
    use http_body_util::combinators::BoxBody;
    BoxBody::new(incoming.map_err(|e| -> Box<dyn std::error::Error + Send + Sync> {
        Box::new(e)
    }))
}

/// TCP 隧道转发统计（用于遥测）
#[derive(Debug, Clone, Default)]
pub struct TunnelStats {
    /// 客户端 → 上游（出站）字节数
    pub bytes_out: u64,
    /// 上游 → 客户端（入站）字节数
    pub bytes_in: u64,
}

/// 转发 TCP 隧道连接
///
/// `max_bytes_per_sec`: 每秒最大字节数（None = 不限制）
pub async fn forward_tcp(
    node: &UpstreamNodeDto,
    inbound: TcpStream,
    timeout: Duration,
    max_bytes_per_sec: Option<u64>,
) -> Result<TunnelStats, ConrogateError> {
    // 使用 DNS 缓存解析地址
    let addrs = crate::dns::global_resolver().resolve(&node.address).await
        .map_err(|e| ConrogateError::UpstreamConnectFailed(format!("DNS resolve: {e}")))?;
    let upstream = tokio::time::timeout(timeout, TcpStream::connect(&addrs[..]))
        .await
        .map_err(|_| ConrogateError::UpstreamTimeout)?
        .map_err(|e| ConrogateError::UpstreamConnectFailed(e.to_string()))?;

    let (mut ri, mut wi) = inbound.into_split();
    let (mut ro, mut wo) = upstream.into_split();

    // 双向转发（可选带宽限制）
    let c2s = async {
        throttled_copy(&mut ri, &mut wo, max_bytes_per_sec).await
    };
    let s2c = async {
        throttled_copy(&mut ro, &mut wi, max_bytes_per_sec).await
    };

    let (bytes_out, bytes_in) = tokio::try_join!(c2s, s2c)
        .map_err(|e| ConrogateError::UpstreamConnectFailed(e.to_string()))?;

    Ok(TunnelStats { bytes_out, bytes_in })
}

/// 带限速的字节流拷贝（返回拷贝字节数）
async fn throttled_copy<R, W>(
    reader: &mut R,
    writer: &mut W,
    max_bytes_per_sec: Option<u64>,
) -> std::io::Result<u64>
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut buf = [0u8; 8192];
    let mut copied: u64 = 0;
    loop {
        let n = reader.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        copied += n as u64;
        writer.write_all(&buf[..n]).await?;
        writer.flush().await?;
        // 带宽限制：按实际传输字节数计算休眠时间
        if let Some(bps) = max_bytes_per_sec {
            if bps > 0 {
                let sleep_us = (n as u64 * 1_000_000) / bps;
                if sleep_us > 0 {
                    tokio::time::sleep(std::time::Duration::from_micros(sleep_us)).await;
                }
            }
        }
    }
    Ok(copied)
}
