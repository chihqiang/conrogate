//! 统一响应结构：所有 API 返回 {"code", "msg", "data", "trace_id"}

use axum::response::{IntoResponse, Response};
use axum::Json;
use conrogate_core::contract::ConrogateError;
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
        code: 0,
        msg: "success".to_string(),
        data: Some(data),
        trace_id: generate_trace_id(),
    })
    .into_response()
}

/// 空数据成功响应
pub fn ok_empty() -> Response {
    Json(UnifiedResponse::<serde_json::Value> {
        code: 0,
        msg: "success".to_string(),
        data: None,
        trace_id: generate_trace_id(),
    })
    .into_response()
}

/// 从 ConrogateError 构建错误响应
pub fn err(e: ConrogateError) -> Response {
    let code = error_code(&e);
    let msg = if e.is_internal() {
        "internal error".to_string()
    } else {
        e.to_string()
    };
    let body = UnifiedResponse::<serde_json::Value> {
        code,
        msg,
        data: None,
        trace_id: generate_trace_id(),
    };
    // 业务错误 HTTP 200（Unauthorized 除外 → 401）
    let http_status = if matches!(e, ConrogateError::Unauthorized) {
        axum::http::StatusCode::UNAUTHORIZED
    } else if e.is_internal() {
        axum::http::StatusCode::INTERNAL_SERVER_ERROR
    } else {
        axum::http::StatusCode::OK
    };
    (http_status, Json(body)).into_response()
}

/// ConrogateError → 错误码
fn error_code(e: &ConrogateError) -> i32 {
    match e {
        ConrogateError::BadRequest(_) => 10001,
        ConrogateError::Unauthorized => 10002,
        ConrogateError::Forbidden => 10003,
        ConrogateError::NotFound(_) => 10004,
        ConrogateError::Conflict(_) => 10005,
        ConrogateError::RateLimited => 10006,
        ConrogateError::PayloadTooLarge => 10007,
        ConrogateError::Business { code, .. } => *code,
        ConrogateError::RouteNotFound(_) => 20001,
        ConrogateError::UpstreamNotFound(_) => 20002,
        ConrogateError::PluginConfigInvalid(_) => 20003,
        ConrogateError::PluginNotFound(_) => 20004,
        ConrogateError::PluginRuntime(_) => 20005,
        ConrogateError::ConfigInvalid(_) => 20006,
        ConrogateError::ConfigConcurrencyConflict => 20007,
        ConrogateError::DatabaseInternal => 30001,
        ConrogateError::DataMapping(_) => 30002,
        ConrogateError::Migration(_) => 30003,
        ConrogateError::NetworkInternal => 40001,
        ConrogateError::UpstreamTimeout => 40002,
        ConrogateError::UpstreamConnectFailed(_) => 40003,
        ConrogateError::UpstreamBadResponse(_) => 40004,
        ConrogateError::ProtocolNotSupported(_) => 40005,
        ConrogateError::GatewayInternal => 40006,
        ConrogateError::CircuitBreakerOpen => 40007,
        ConrogateError::Limited => 40008,
        ConrogateError::RetryExhausted(_) => 40009,
        ConrogateError::ConfigLoad(_) => 50001,
        ConrogateError::Init(_) => 50002,
        ConrogateError::Internal(_) => 59999,
    }
}

fn generate_trace_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{:032x}", nanos)
}
