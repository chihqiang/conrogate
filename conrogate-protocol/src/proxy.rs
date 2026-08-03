//! 代理转发：HTTP 请求转发 + 响应回传 + TCP 双向转发。

use bytes::Bytes;
use conrogate_contract::dto::UpstreamNodeDto;
use conrogate_contract::ConrogateError;
use http::Request;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper_rustls::HttpsConnector;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::Client;
use std::time::Duration;
use tokio::net::TcpStream;

/// 统一请求体类型：BoxBody 兼容缓冲模式（`Full<Bytes>`）和流式模式（`Incoming`）
pub type ReqBody =
    http_body_util::combinators::BoxBody<Bytes, Box<dyn std::error::Error + Send + Sync>>;

/// 出站 HTTP 客户端：支持 http:// 与 https://（TLS）上游
pub type HttpClient = Client<HttpsConnector<HttpConnector>, ReqBody>;

/// 规范化上游地址：支持显式 scheme（`https://host:port`）；缺省按 http。
/// 全项目统一使用该函数，避免硬编码 `http://` 导致 https 上游拼出非法 URI。
pub fn upstream_addr(node: &UpstreamNodeDto) -> String {
    if node.address.contains("://") {
        node.address.clone()
    } else {
        format!("http://{}", node.address)
    }
}

/// 上游 scheme（http/https），用于 x-forwarded-proto 注入
pub fn upstream_scheme(node: &UpstreamNodeDto) -> &'static str {
    if node.address.starts_with("https://") {
        "https"
    } else {
        "http"
    }
}

/// 代理转发结果（缓冲模式：响应体已 collect 进内存）
pub struct ProxyResult {
    pub status: http::StatusCode,
    pub headers: http::HeaderMap,
    pub body: Bytes,
}

/// 代理转发结果（流式模式：响应体保持流式，不载入内存）
pub struct ProxyStreamResult {
    pub status: http::StatusCode,
    pub headers: http::HeaderMap,
    pub body: Incoming,
}

/// 转发 HTTP 请求到上游节点（缓冲模式：body 已在内存中）
pub async fn forward_http(
    client: &HttpClient,
    node: &UpstreamNodeDto,
    req: Request<ReqBody>,
    timeout: Duration,
) -> Result<ProxyResult, ConrogateError> {
    // 整个响应（头 + 体）受 total 超时约束：上游发完响应头后中途停滞也会超时，避免挂死
    let result = tokio::time::timeout(timeout, async {
        let (status, headers, body) = forward_common(client, node, req, timeout).await?;
        let body_bytes = body
            .collect()
            .await
            .map_err(|e| ConrogateError::UpstreamBadResponse(e.to_string()))?
            .to_bytes();
        Ok::<_, ConrogateError>(ProxyResult {
            status,
            headers,
            body: body_bytes,
        })
    })
    .await
    .map_err(|_| ConrogateError::UpstreamTimeout)??;
    Ok(result)
}

/// 转发 HTTP 请求到上游节点（流式模式：请求体与响应体均以流透传，不载入内存）
pub async fn forward_http_stream(
    client: &HttpClient,
    node: &UpstreamNodeDto,
    req: Request<ReqBody>,
    timeout: Duration,
) -> Result<ProxyStreamResult, ConrogateError> {
    let (status, headers, body) = forward_common(client, node, req, timeout).await?;
    Ok(ProxyStreamResult {
        status,
        headers,
        body,
    })
}

/// 内部转发逻辑：返回未 collect 的响应体（由调用方决定缓冲或流式）
async fn forward_common(
    client: &HttpClient,
    node: &UpstreamNodeDto,
    req: Request<ReqBody>,
    timeout: Duration,
) -> Result<(http::StatusCode, http::HeaderMap, Incoming), ConrogateError> {
    // 地址支持显式 scheme（https://host:port）；缺省按 http
    let addr = upstream_addr(node);

    let (method, uri, headers, body) = (
        req.method().clone(),
        req.uri().clone(),
        req.headers().clone(),
        req.into_body(),
    );

    let path_and_query = uri.path_and_query().map(|p| p.as_str()).unwrap_or("/");
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
    Ok((parts.status, parts.headers, body))
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
    BoxBody::new(incoming.map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) }))
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
    let addrs = crate::dns::global_resolver()
        .resolve(&node.address)
        .await
        .map_err(|e| ConrogateError::UpstreamConnectFailed(format!("DNS resolve: {e}")))?;
    let upstream = tokio::time::timeout(timeout, TcpStream::connect(&addrs[..]))
        .await
        .map_err(|_| ConrogateError::UpstreamTimeout)?
        .map_err(|e| ConrogateError::UpstreamConnectFailed(e.to_string()))?;

    let (mut ri, mut wi) = inbound.into_split();
    let (mut ro, mut wo) = upstream.into_split();

    // 双向转发（可选带宽限制）
    let c2s = async { throttled_copy(&mut ri, &mut wo, max_bytes_per_sec).await };
    let s2c = async { throttled_copy(&mut ro, &mut wi, max_bytes_per_sec).await };

    let (bytes_out, bytes_in) = tokio::try_join!(c2s, s2c)
        .map_err(|e| ConrogateError::UpstreamConnectFailed(e.to_string()))?;

    Ok(TunnelStats {
        bytes_out,
        bytes_in,
    })
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
            if let Some(sleep_us) = (n as u64 * 1_000_000).checked_div(bps).filter(|&us| us > 0) {
                tokio::time::sleep(std::time::Duration::from_micros(sleep_us)).await;
            }
        }
    }
    Ok(copied)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// 启动一个简单的上游 HTTP 服务器，返回 chunked 响应体
    async fn spawn_upstream() -> (String, std::net::SocketAddr) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((mut sock, _)) = listener.accept().await {
                let mut buf = [0u8; 2048];
                let _ = sock.read(&mut buf).await;
                let _ = sock
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n5\r\nworld\r\n0\r\n\r\n",
                    )
                    .await;
                let _ = sock.flush().await;
            }
        });
        ("127.0.0.1".to_string(), addr)
    }

    fn test_client() -> HttpClient {
        let connector = hyper_rustls::HttpsConnectorBuilder::new()
            .with_native_roots()
            .expect("native roots")
            .https_or_http();
        Client::builder(hyper_util::rt::TokioExecutor::new())
            .build(connector.enable_http1().enable_http2().build())
    }

    /// 流式转发：响应体以 Incoming 流式返回，不提前 collect（docs/10 §11.1 响应侧流式）
    #[tokio::test]
    async fn forward_http_stream_returns_streamed_body() {
        let (host, addr) = spawn_upstream().await;
        let node = UpstreamNodeDto {
            id: 1,
            upstream_id: 1,
            address: format!("{host}:{}", addr.port()),
            weight: 1,
            enabled: true,
        };
        let req = Request::builder()
            .uri("http://upstream/")
            .body(body_from_bytes(Bytes::new()))
            .unwrap();

        let result = forward_http_stream(&test_client(), &node, req, Duration::from_secs(5))
            .await
            .expect("forward");
        // 流式结果携带 Incoming 体（不载入内存的强类型保证）
        let body: Incoming = result.body;
        let collected = body
            .collect()
            .await
            .expect("collect streamed body")
            .to_bytes();
        assert_eq!(&collected[..], b"helloworld");
    }

    /// 缓冲模式：上游发完响应头后停滞 → 整个响应受 total 超时约束，不挂死
    #[tokio::test]
    async fn forward_http_times_out_on_stalled_body() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((mut sock, _)) = listener.accept().await {
                let mut buf = [0u8; 2048];
                let _ = sock.read(&mut buf).await;
                // 只发响应头不发体，然后停滞
                let _ = sock
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\n")
                    .await;
                let _ = sock.flush().await;
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        });
        let node = UpstreamNodeDto {
            id: 1,
            upstream_id: 1,
            address: format!("127.0.0.1:{}", addr.port()),
            weight: 1,
            enabled: true,
        };
        let req = Request::builder()
            .uri("http://upstream/")
            .body(body_from_bytes(Bytes::new()))
            .unwrap();

        let start = std::time::Instant::now();
        let result = forward_http(&test_client(), &node, req, Duration::from_millis(300)).await;
        assert!(matches!(result, Err(ConrogateError::UpstreamTimeout)));
        assert!(start.elapsed() < std::time::Duration::from_secs(2), "应在超时内返回");
    }
}
