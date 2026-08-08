//! 协议标识与路由匹配类型。

use serde::{Deserialize, Serialize};

/// 数据面支持的协议标识，随协议扩展增量追加
///
/// 反序列化兼容 `"tcp"` 作为 `tcp_tunnel` 的别名（手动实现 Deserialize）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, utoipa::ToSchema, Default)]
pub enum ProtocolId {
    /// HTTP/1.1 + HTTP/2
    #[default]
    #[serde(rename = "http")]
    Http,
    /// WebSocket（HTTP 升级）
    #[serde(rename = "websocket")]
    WebSocket,
    /// TCP 隧道
    #[serde(rename = "tcp_tunnel")]
    TcpTunnel,
}

impl<'de> Deserialize<'de> for ProtocolId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "http" => Ok(Self::Http),
            "websocket" => Ok(Self::WebSocket),
            "tcp_tunnel" | "tcp" => Ok(Self::TcpTunnel),
            _ => Err(serde::de::Error::unknown_variant(
                &s,
                &["http", "websocket", "tcp_tunnel", "tcp"],
            )),
        }
    }
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
    /// 缺省为空列表（API 允许省略，兼容无 header 条件路由）
    #[serde(default)]
    pub headers: Vec<HeaderMatch>,
    /// 缺省为空列表（API 允许省略，兼容无 query 条件路由）
    #[serde(default)]
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
    /// 从 HTTP 请求构造。
    ///
    /// `include_headers` 控制是否解析请求头为 `Vec<(String, String)>`：
    /// 仅路由表存在 header 匹配条件时需要（热路径按需构造，避免无谓分配）。
    /// `query_params` 始终解析：插件上下文需要完整的查询参数。
    pub fn from_http_request(
        method: &http::Method,
        uri: &http::Uri,
        headers: &http::HeaderMap,
        include_headers: bool,
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

        let header_vec: Vec<(String, String)> = if include_headers {
            headers
                .iter()
                .filter_map(|(name, value)| {
                    let k = name.as_str().to_string();
                    let v = value.to_str().ok()?.to_string();
                    Some((k, v))
                })
                .collect()
        } else {
            Vec::new()
        };

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

#[cfg(test)]
mod tests {
    use super::*;

    /// 兼容省略 headers/query_params 的路由条件（serde default）
    #[test]
    fn test_route_conditions_optional_fields_default() {
        let conds: RouteMatchConditions =
            serde_json::from_str(r#"{"path":{"prefix":"/api"}}"#).expect("缺省字段应解析成功");
        match conds.path {
            PathMatch::Prefix(p) => assert_eq!(p, "/api"),
            other => panic!("unexpected path match: {other:?}"),
        }
        assert!(conds.headers.is_empty());
        assert!(conds.query_params.is_empty());
        assert!(conds.methods.is_none());
        assert!(conds.host.is_none());
    }

    /// 协议反序列化兼容 "tcp" 别名
    #[test]
    fn test_protocol_id_deserialize_aliases() {
        assert_eq!(
            serde_json::from_str::<ProtocolId>(r#""tcp""#).unwrap(),
            ProtocolId::TcpTunnel
        );
        assert_eq!(
            serde_json::from_str::<ProtocolId>(r#""tcp_tunnel""#).unwrap(),
            ProtocolId::TcpTunnel
        );
        assert_eq!(
            serde_json::from_str::<ProtocolId>(r#""http""#).unwrap(),
            ProtocolId::Http
        );
        assert_eq!(
            serde_json::from_str::<ProtocolId>(r#""websocket""#).unwrap(),
            ProtocolId::WebSocket
        );
        assert!(serde_json::from_str::<ProtocolId>(r#""ftp""#).is_err());
    }

    /// 序列化仍输出规范 snake_case（TcpTunnel → tcp_tunnel）
    #[test]
    fn test_protocol_id_serialize_canonical() {
        assert_eq!(
            serde_json::to_string(&ProtocolId::TcpTunnel).unwrap(),
            r#""tcp_tunnel""#
        );
        assert_eq!(
            serde_json::to_string(&ProtocolId::WebSocket).unwrap(),
            r#""websocket""#
        );
    }
}
