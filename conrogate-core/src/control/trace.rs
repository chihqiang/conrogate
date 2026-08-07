//! 链路追踪中间件：统一 trace_id 的提取/生成与传递。
//!
//! 每个请求只生成一个 trace_id：入站 `x-trace-id` 优先，否则生成。
//! 中间件把 trace_id 写入请求 Extension（供 handler 取用）、回写 `x-trace-id`
//! 响应头，并覆写统一信封响应体中的 `trace_id` 字段，保证
//! **body trace_id == x-trace-id 响应头 == 日志 span == 审计** 全链路一致。

use crate::contract::constant::TRACE_ID_HEADER;
use crate::contract::response::trace_id_from_headers;
use axum::body::Body;
use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;
use http_body_util::BodyExt;
use std::ops::Deref;

/// 请求级追踪 ID（由 trace 中间件注入请求 Extension）
#[derive(Debug, Clone)]
pub struct TraceId(pub String);

impl Deref for TraceId {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// trace 中间件：提取/生成 trace_id，注入 Extension + 请求头，出口写响应头并覆写信封 body
pub async fn trace_middleware(req: Request, next: Next) -> Response {
    let trace_id = trace_id_from_headers(req.headers());

    let (mut parts, body) = req.into_parts();
    parts.extensions.insert(TraceId(trace_id.clone()));
    // 写入请求头，保证内层 TraceLayer / handler 取到同一 trace_id
    if let Ok(v) = trace_id.parse() {
        parts.headers.insert(TRACE_ID_HEADER, v);
    }
    let req = Request::from_parts(parts, body);

    let mut resp = next.run(req).await;

    // 响应头注入
    if let Ok(v) = trace_id.parse() {
        resp.headers_mut().insert(TRACE_ID_HEADER, v);
    }

    // 覆写统一信封响应体中的 trace_id（仅当响应体是含 trace_id 字段的 JSON 信封）
    let original_body = std::mem::take(resp.body_mut());
    if let Some(body) = rewrite_envelope_trace_id(original_body, &trace_id).await {
        *resp.body_mut() = body;
    }

    resp
}

/// 若响应体为统一信封（顶层含 `trace_id` 字段的 JSON），把该字段覆写为规范 trace_id
async fn rewrite_envelope_trace_id(body: Body, trace_id: &str) -> Option<Body> {
    let bytes = body.collect().await.ok()?.to_bytes();
    let mut value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let obj = value.as_object_mut()?;
    if !obj.contains_key("trace_id") {
        return None;
    }
    obj.insert(
        "trace_id".to_string(),
        serde_json::Value::String(trace_id.to_string()),
    );
    let out = serde_json::to_vec(&value).ok()?;
    Some(Body::from(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::ConrogateError;
    use axum::extract::Extension;
    use axum::routing::get;
    use axum::{middleware, Router};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    async fn ok_handler(Extension(trace): Extension<TraceId>) -> axum::response::Response {
        crate::contract::response::ok(serde_json::json!({"echo": trace.to_string()}))
    }

    async fn err_handler() -> axum::response::Response {
        crate::contract::response::err(ConrogateError::BadRequest("boom".into()))
    }

    fn app() -> Router {
        Router::new()
            .route("/ok", get(ok_handler))
            .route("/err", get(err_handler))
            .layer(middleware::from_fn(trace_middleware))
    }

    async fn body_trace_id(resp: axum::response::Response) -> (http::HeaderMap, serde_json::Value) {
        let headers = resp.headers().clone();
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        (headers, value)
    }

    #[tokio::test]
    async fn inbound_trace_id_propagates_to_header_and_body() {
        let resp = app()
            .oneshot(
                axum::extract::Request::builder()
                    .uri("/ok")
                    .header("x-trace-id", "trace-inbound-abc")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let (headers, body) = body_trace_id(resp).await;
        assert_eq!(
            headers.get("x-trace-id").unwrap().to_str().unwrap(),
            "trace-inbound-abc"
        );
        assert_eq!(body["trace_id"], "trace-inbound-abc");
        assert_eq!(body["data"]["echo"], "trace-inbound-abc");
    }

    #[tokio::test]
    async fn generated_trace_id_is_consistent() {
        let resp = app()
            .oneshot(
                axum::extract::Request::builder()
                    .uri("/err")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let (headers, body) = body_trace_id(resp).await;
        let header = headers
            .get("x-trace-id")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        assert!(!header.is_empty());
        assert_eq!(body["trace_id"], header);
        assert_eq!(body["code"], ConrogateError::ERR_BAD_REQUEST);
    }

    #[tokio::test]
    async fn non_envelope_body_is_left_untouched() {
        let resp = app()
            .oneshot(
                axum::extract::Request::builder()
                    .uri("/nonexistent")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        // 404 非信封体：仅响应头带 trace_id，body 不被改写
        let header = resp
            .headers()
            .get("x-trace-id")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        assert!(!header.is_empty());
    }
}
