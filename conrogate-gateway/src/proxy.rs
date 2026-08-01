//! 代理转发：HTTP 请求转发 + 响应回传。

use bytes::Bytes;
use conrogate_contract::dto::UpstreamNodeDto;
use conrogate_contract::ConrogateError;
use http::Request;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioIo;
use std::time::Duration;
use tokio::net::TcpStream;

/// 统一请求体类型：BoxBody 兼容缓冲模式（Full<Bytes>）和流式模式（Incoming）
pub type ReqBody = http_body_util::combinators::BoxBody<Bytes, Box<dyn std::error::Error + Send + Sync>>;

/// 代理转发结果
pub struct ProxyResult {
    pub status: http::StatusCode,
    pub headers: http::HeaderMap,
    pub body: Bytes,
}

/// 转发 HTTP 请求到上游节点（缓冲模式：body 已在内存中）
pub async fn forward_http(
    client: &Client<HttpConnector, ReqBody>,
    node: &UpstreamNodeDto,
    req: Request<ReqBody>,
    timeout: Duration,
) -> Result<ProxyResult, ConrogateError> {
    forward_internal(client, node, req, timeout).await
}

/// 转发 HTTP 请求到上游节点（流式模式：body 以 BoxBody 包装 Incoming，不提前 collect）
pub async fn forward_http_stream(
    client: &Client<HttpConnector, ReqBody>,
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
    client: &Client<HttpConnector, ReqBody>,
    node: &UpstreamNodeDto,
    req: Request<ReqBody>,
    timeout: Duration,
) -> Result<ProxyResult, ConrogateError> {
    let addr = format!("http://{}", node.address);

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

/// 转发 TCP 隧道连接
pub async fn forward_tcp(
    node: &UpstreamNodeDto,
    inbound: TcpStream,
    timeout: Duration,
) -> Result<(), ConrogateError> {
    let upstream = tokio::time::timeout(timeout, TcpStream::connect(&node.address))
        .await
        .map_err(|_| ConrogateError::UpstreamTimeout)?
        .map_err(|e| ConrogateError::UpstreamConnectFailed(e.to_string()))?;

    let (mut ri, mut wi) = inbound.into_split();
    let (mut ro, mut wo) = upstream.into_split();

    // 双向转发
    let c2s = async {
        tokio::io::copy(&mut ri, &mut wo).await
    };
    let s2c = async {
        tokio::io::copy(&mut ro, &mut wi).await
    };

    tokio::try_join!(c2s, s2c)
        .map_err(|e| ConrogateError::UpstreamConnectFailed(e.to_string()))?;

    Ok(())
}
