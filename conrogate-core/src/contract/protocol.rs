//! 协议标识与路由匹配类型。

use serde::{Deserialize, Serialize};

/// 数据面支持的协议标识，随协议扩展增量追加
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolId {
    /// HTTP/1.1 + HTTP/2
    #[default]
    Http,
    /// WebSocket（HTTP 升级）
    WebSocket,
    /// TCP 隧道
    TcpTunnel,
}

impl std::fmt::Display for ProtocolId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http => write!(f, "http"),
            Self::WebSocket => write!(f, "websocket"),
            Self::TcpTunnel => write!(f, "tcp_tunnel"),
        }
    }
}

impl std::str::FromStr for ProtocolId {
    type Err = crate::contract::ConrogateError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "http" => Ok(Self::Http),
            "websocket" => Ok(Self::WebSocket),
            "tcp_tunnel" | "tcp" => Ok(Self::TcpTunnel),
            _ => Err(crate::contract::ConrogateError::ProtocolNotSupported(
                s.to_string(),
            )),
        }
    }
}

/// 路径匹配方式
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PathMatch {
    Prefix(String),
    Exact(String),
    Regex(String),
}

/// 通用匹配操作符
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MatchOp {
    Exact,
    Prefix,
    Regex,
    NotEmpty,
}

/// Header 匹配条件
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct HeaderMatch {
    pub key: String,
    pub op: MatchOp,
    pub value: String,
}

/// Query 参数匹配条件
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct QueryMatch {
    pub key: String,
    pub op: MatchOp,
    pub value: String,
}

/// 路由匹配条件集合（多维匹配，全部条件 AND 关系）
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct RouteMatchConditions {
    pub path: PathMatch,
    pub methods: Option<Vec<String>>,
    pub host: Option<String>,
    pub headers: Vec<HeaderMatch>,
    pub query_params: Vec<QueryMatch>,
}

impl Default for RouteMatchConditions {
    fn default() -> Self {
        Self {
            path: PathMatch::Prefix("/".into()),
            methods: None,
            host: None,
            headers: vec![],
            query_params: vec![],
        }
    }
}

/// 路由匹配输入信息（从入站请求构造）
#[derive(Debug, Clone)]
pub struct RouteMatchInfo {
    pub path: String,
    pub method: Option<String>,
    pub host: Option<String>,
    pub headers: Vec<(String, String)>,
    pub query_params: Vec<(String, String)>,
}

impl RouteMatchInfo {
    /// 从 HTTP 请求构造
    pub fn from_http_request(
        method: &http::Method,
        uri: &http::Uri,
        headers: &http::HeaderMap,
    ) -> Self {
        let path = uri.path().to_string();

        let method_str = if method == http::Method::OPTIONS {
            Some("OPTIONS".to_string())
        } else {
            Some(method.as_str().to_string())
        };

        let host = headers
            .get(http::header::HOST)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let header_vec: Vec<(String, String)> = headers
            .iter()
            .filter_map(|(name, value)| {
                let k = name.as_str().to_string();
                let v = value.to_str().ok()?.to_string();
                Some((k, v))
            })
            .collect();

        let query_vec: Vec<(String, String)> = uri
            .query()
            .map(|q| {
                q.split('&')
                    .filter_map(|pair| {
                        let mut split = pair.splitn(2, '=');
                        let k = split.next()?.to_string();
                        let v = split.next().unwrap_or("").to_string();
                        Some((k, v))
                    })
                    .collect()
            })
            .unwrap_or_default();

        Self {
            path,
            method: method_str,
            host,
            headers: header_vec,
            query_params: query_vec,
        }
    }

    /// 从隧道连接信息构造
    pub fn from_tunnel(listen_addr: &str, sni: Option<&str>) -> Self {
        Self {
            path: listen_addr.to_string(),
            method: None,
            host: sni.map(|s| s.to_string()),
            headers: vec![],
            query_params: vec![],
        }
    }
}
