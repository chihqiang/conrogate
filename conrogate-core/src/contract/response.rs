//! 统一响应结构：所有接口（控制面 API / 数据面网关错误）返回 `{"code","msg","data","trace_id"}`。
//!
//! trace_id 生命周期：请求入口提取/生成一次（见 `trace_id_from_headers`），
//! 贯穿响应信封、`x-trace-id` 响应头、日志 span 与审计，保证一次请求可端到端追踪。

use crate::contract::constant::TRACE_ID_HEADER;
use crate::contract::ConrogateError;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

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

/// 生成追踪 ID（保持 UUID v4 格式）
///
/// 使用快速生成器（见 [`FastIdGen`]），不引入每请求的 getrandom 系统调用。
pub fn generate_trace_id() -> String {
    fast_id().generate()
}

/// 生成 `[0, limit)` 范围内的伪随机数（splitmix64，无系统调用）。
/// 用于重试退避抖动等热路径，替代 `Uuid::new_v4()` 取随机。
pub fn jitter(limit: u64) -> u64 {
    fast_id().jitter(limit)
}

// ── 进程级快速 ID 生成器 ──

const HEX_CHARS: &[u8; 16] = b"0123456789abcdef";

/// 进程级快速 ID 生成器（输出保持 UUID v4 格式）。
///
/// 相比 `uuid::Uuid::new_v4()`（每次调用触发一次 getrandom 系统调用 + 通用格式化），
/// 这里用「进程随机种子 + 原子计数器 + 时间戳」组合：
/// 唯一性由随机种子（跨进程防碰撞）与单调计数器（进程内严格递增）保证，
/// 每次生成仅一次原子递增 + 一次字节格式化，零系统调用。
struct FastIdGen {
    seed: u64,
    counter: AtomicU64,
}

impl FastIdGen {
    fn new() -> Self {
        let seed = now_millis() ^ rand::random::<u64>();
        Self {
            seed: seed.max(1),
            counter: AtomicU64::new(0),
        }
    }

    /// 生成 UUID v4 格式 ID（36 字符）。
    /// 高 64 位 = 进程随机种子，低 64 位 = 时间戳(44bit) | 计数器(20bit)。
    fn generate(&self) -> String {
        let ts_44 = now_millis() & 0xF_FFFF_FFFF;
        let ctr = self.counter.fetch_add(1, Ordering::Relaxed);
        let low = (ts_44 << 20) | (ctr & 0xF_FFFF);

        let mut b = [0u8; 16];
        b[..8].copy_from_slice(&self.seed.to_be_bytes());
        b[8..].copy_from_slice(&low.to_be_bytes());
        // UUID v4 版本位（4xxx）+ RFC 4122 变体位（8/9/a/b）
        b[6] = (b[6] & 0x0f) | 0x40;
        b[8] = (b[8] & 0x3f) | 0x80;

        let mut out = String::with_capacity(36);
        for i in 0..16 {
            if matches!(i, 4 | 6 | 8 | 10) {
                out.push('-');
            }
            out.push(HEX_CHARS[(b[i] >> 4) as usize] as char);
            out.push(HEX_CHARS[(b[i] & 0x0f) as usize] as char);
        }
        out
    }

    /// splitmix64 伪随机数：`[0, limit)`，无系统调用，适合重试抖动
    fn jitter(&self, limit: u64) -> u64 {
        let mut x = self.seed ^ self.counter.fetch_add(1, Ordering::Relaxed);
        x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
        x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        (x ^ (x >> 31)) % limit
    }
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn fast_id() -> &'static FastIdGen {
    static GEN: OnceLock<FastIdGen> = OnceLock::new();
    GEN.get_or_init(FastIdGen::new)
}
