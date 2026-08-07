//! 统一响应结构：所有接口（控制面 API / 数据面网关错误）返回 `{"code","msg","data","trace_id"}`。
//!
//! trace_id 生命周期：请求入口提取/生成一次（见 `trace_id_from_headers`），
//! 贯穿响应信封、`x-trace-id` 响应头、日志 span 与审计，保证一次请求可端到端追踪。

use crate::contract::constant::TRACE_ID_HEADER;
use crate::contract::ConrogateError;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

/// 统一响应包装
#[derive(Debug, Serialize)]
pub struct UnifiedResponse<T: Serialize> {
    pub code: i32,
    pub msg: String,
    pub data: Option<T>,
    pub trace_id: String,
}

/// 成功响应包装
pub fn ok<T: Serialize>(data: T) -> Response {
    Json(UnifiedResponse {
        code: ConrogateError::OK,
        msg: "success".to_string(),
        data: Some(data),
        trace_id: generate_trace_id(),
    })
    .into_response()
}

/// 空数据成功响应
pub fn ok_empty() -> Response {
    Json(UnifiedResponse::<serde_json::Value> {
        code: ConrogateError::OK,
        msg: "success".to_string(),
        data: None,
        trace_id: generate_trace_id(),
    })
    .into_response()
}

/// 从 ConrogateError 构建错误响应
pub fn err(e: ConrogateError) -> Response {
    // 业务错误 HTTP 200（Unauthorized 除外 → 401）
    let http_status = if matches!(e, ConrogateError::Unauthorized) {
        axum::http::StatusCode::UNAUTHORIZED
    } else if e.is_internal() {
        axum::http::StatusCode::INTERNAL_SERVER_ERROR
    } else {
        axum::http::StatusCode::OK
    };
    (http_status, Json(error_body(e.code(), message(&e)))).into_response()
}

/// 构造统一错误响应体（控制面信封）：`{"code","msg","data":null,"trace_id"}`
pub fn error_body(code: i32, msg: impl Into<String>) -> serde_json::Value {
    serde_json::json!({
        "code": code,
        "msg": msg.into(),
        "data": null,
        "trace_id": generate_trace_id(),
    })
}

/// 构造统一错误响应体（显式 trace_id；HTTP/WS 拦截、插件拒绝、网关错误场景，
/// 保证 body 与 `x-trace-id` 响应头一致）
pub fn error_body_with_trace(
    trace_id: &str,
    code: i32,
    msg: impl Into<String>,
) -> serde_json::Value {
    serde_json::json!({
        "code": code,
        "msg": msg.into(),
        "data": null,
        "trace_id": trace_id,
    })
}

/// 对外展示的错误信息（内部错误不暴露细节）
fn message(e: &ConrogateError) -> String {
    if e.is_internal() {
        "internal error".to_string()
    } else {
        e.to_string()
    }
}

/// 从请求头提取 trace_id：优先入站 `x-trace-id`，否则生成
pub fn trace_id_from_headers(headers: &axum::http::HeaderMap) -> String {
    headers
        .get(TRACE_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(generate_trace_id)
}

/// 生成追踪 ID（UUID v4）
pub fn generate_trace_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
