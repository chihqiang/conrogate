//! 代理转发：HTTP 请求转发 + 响应回传。

use bytes::Bytes;
use conrogate_contract::dto::UpstreamNodeDto;
use conrogate_contract::ConrogateError;
use http::Request;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioIo;
use std::time::Duration;
use tokio::net::TcpStream;

/// 代理转发结果
pub struct ProxyResult {
    pub status: http::StatusCode,
    pub headers: http::HeaderMap,
    pub body: Bytes,
}

/// 转发 HTTP 请求到上游节点
pub async fn forward_http(
    client: &Client<hyper_util::client::legacy::connect::HttpConnector, Full<Bytes>>,
    node: &UpstreamNodeDto,
    req: Request<Full<Bytes>>,
    timeout: Duration,
) -> Result<ProxyResult, ConrogateError> {
    let addr = format!("http://{}", node.address);

    // 构建到上游的请求
    let (method, uri, headers, body) = (
        req.method().clone(),
        req.uri().clone(),
        req.headers().clone(),
        req.into_body(),
    );

    // 替换 URI 的 host 部分
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
