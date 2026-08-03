//! 认证中间件：Bearer Token 校验 + RBAC 角色控制。
//! Token 格式：operator:secret:role（role = viewer / operator / admin）

use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;

/// 认证配置
#[derive(Clone)]
pub struct AuthState {
    pub token: String,
}

/// 用户角色
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd)]
pub enum Role {
    Viewer,
    Operator,
    Admin,
}

impl Role {
    /// 从字符串解析角色
    pub fn parse_role(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "admin" => Self::Admin,
            "operator" => Self::Operator,
            _ => Self::Viewer,
        }
    }

    /// 检查是否有足够权限
    pub fn has_permission(&self, required: Self) -> bool {
        self >= &required
    }
}

/// 解析 token 格式 operator:secret:role，返回 (operator, role)
fn parse_token(token: &str) -> Option<(&str, &str, Role)> {
    let parts: Vec<&str> = token.split(':').collect();
    if parts.len() != 3 {
        return None;
    }
    let role = Role::parse_role(parts[2]);
    Some((parts[0], parts[1], role))
}

/// 构建 401 统一错误体（docs/12 §10.5：code=10002）
fn unauthorized_response() -> Response {
    use std::time::{SystemTime, UNIX_EPOCH};
    let trace_id = format!(
        "{:032x}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({
            "code": 10002,
            "msg": "unauthorized",
            "data": null,
            "trace_id": trace_id,
        })),
    )
        .into_response()
}

/// Bearer Token 认证中间件
pub async fn auth_middleware(State(state): State<AuthState>, req: Request, next: Next) -> Response {
    // 如果未配置 token，跳过认证
    if state.token.is_empty() {
        return next.run(req).await;
    }

    let auth_header = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    match auth_header {
        Some(header) if header.starts_with("Bearer ") => {
            let token = &header[7..];
            // 校验 token 完全匹配
            if token != state.token {
                return unauthorized_response();
            }
            // 解析角色并注入到请求扩展中
            let (role, operator) = if let Some((op, _secret, r)) = parse_token(token) {
                (r, op.to_string())
            } else {
                (Role::Viewer, "unknown".to_string())
            };
            // 分拆请求注入扩展
            let (mut parts, body) = req.into_parts();
            parts.extensions.insert(role);
            parts.extensions.insert(operator);
            let req = Request::from_parts(parts, body);
            next.run(req).await
        }
        _ => unauthorized_response(),
    }
}
