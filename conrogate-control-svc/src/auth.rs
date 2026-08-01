//! 认证中间件：Bearer Token 校验 + RBAC 角色控制。
//! Token 格式：operator:secret:role（role = viewer / operator / admin）

use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::Response;

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
    pub fn from_str(s: &str) -> Self {
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
    let role = Role::from_str(parts[2]);
    Some((parts[0], parts[1], role))
}

/// Bearer Token 认证中间件
pub async fn auth_middleware(
    State(state): State<AuthState>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // 如果未配置 token，跳过认证
    if state.token.is_empty() {
        return Ok(next.run(req).await);
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
                return Err(StatusCode::UNAUTHORIZED);
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
            Ok(next.run(req).await)
        }
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}
