//! 路由匹配引擎：多维条件匹配 + 优先级排序。

use conrogate_contract::dto::{RouteDto, RouteSnapshot};
use conrogate_contract::gateway::RouteLookup;
use conrogate_contract::protocol::{
    HeaderMatch, MatchOp, PathMatch, ProtocolId, QueryMatch, RouteMatchConditions,
    RouteMatchInfo,
};
use conrogate_contract::ConrogateError;
use std::collections::HashMap;
use std::sync::RwLock;

/// 路由匹配引擎
pub struct RouteMatcher {
    // 按协议分组存储路由快照
    // Vec 按 priority 降序排列
    routes: RwLock<HashMap<ProtocolId, Vec<RouteEntry>>>,
}

struct RouteEntry {
    snapshot: RouteSnapshot,
    conditions: RouteMatchConditions,
    priority: i32,
}

impl RouteMatcher {
    pub fn new() -> Self {
        Self {
            routes: RwLock::new(HashMap::new()),
        }
    }

    /// 从路由 DTO 列表加载路由表
    pub fn load(&self, dtos: Vec<RouteDto>) {
        let mut routes = self.routes.write().unwrap();
        routes.clear();

        for dto in dtos {
            if !dto.enabled {
                continue;
            }

            let entry = RouteEntry {
                conditions: dto.match_conditions.clone(),
                priority: dto.priority,
                snapshot: RouteSnapshot {
                    id: dto.id,
                    protocol: dto.protocol,
                    upstream_id: dto.upstream_id,
                    host_header: dto.host_header.clone(),
                    allow_retry_non_idempotent: dto.allow_retry_non_idempotent,
                    plugin_chain: vec![],
                },
            };

            routes
                .entry(dto.protocol)
                .or_default()
                .push(entry);
        }

        // 每个协议组按 priority 降序排列
        for entries in routes.values_mut() {
            entries.sort_by(|a, b| b.priority.cmp(&a.priority));
        }
    }

    /// 匹配路由
    pub fn match_route(
        &self,
        protocol: ProtocolId,
        info: &RouteMatchInfo,
    ) -> Option<RouteSnapshot> {
        let routes = self.routes.read().unwrap();
        let entries = routes.get(&protocol)?;

        for entry in entries {
            if Self::matches(&entry.conditions, info) {
                return Some(entry.snapshot.clone());
            }
        }
        None
    }

    /// 检查单个路由条件是否匹配
    fn matches(conditions: &RouteMatchConditions, info: &RouteMatchInfo) -> bool {
        // 1. 路径匹配
        if !Self::match_path(&conditions.path, &info.path) {
            return false;
        }

        // 2. 方法匹配
        if let Some(ref methods) = conditions.methods {
            if let Some(ref method) = info.method {
                if !methods.iter().any(|m| m.eq_ignore_ascii_case(method)) {
                    return false;
                }
            } else {
                return false;
            }
        }

        // 3. Host 匹配
        if let Some(ref host) = conditions.host {
            match &info.host {
                Some(h) if h.eq_ignore_ascii_case(host) => {}
                _ => return false,
            }
        }

        // 4. Header 匹配
        for header_match in &conditions.headers {
            if !Self::match_header(header_match, &info.headers) {
                return false;
            }
        }

        // 5. Query 参数匹配
        for query_match in &conditions.query_params {
            if !Self::match_query(query_match, &info.query_params) {
                return false;
            }
        }

        true
    }

    fn match_path(path_match: &PathMatch, request_path: &str) -> bool {
        match path_match {
            PathMatch::Exact(p) => request_path == p,
            PathMatch::Prefix(p) => request_path.starts_with(p),
            PathMatch::Regex(pattern) => {
                // 简易正则匹配（避免引入 regex crate 的编译时间）
                // 实际生产环境应使用 regex crate
                simple_regex_match(pattern, request_path)
            }
        }
    }

    fn match_header(hm: &HeaderMatch, headers: &[(String, String)]) -> bool {
        match hm.op {
            MatchOp::Exact => headers
                .iter()
                .any(|(k, v)| k.eq_ignore_ascii_case(&hm.key) && v == &hm.value),
            MatchOp::Prefix => headers
                .iter()
                .any(|(k, v)| k.eq_ignore_ascii_case(&hm.key) && v.starts_with(&hm.value)),
            MatchOp::Regex => headers.iter().any(|(k, v)| {
                k.eq_ignore_ascii_case(&hm.key) && simple_regex_match(&hm.value, v)
            }),
            MatchOp::NotEmpty => headers
                .iter()
                .any(|(k, v)| k.eq_ignore_ascii_case(&hm.key) && !v.is_empty()),
        }
    }

    fn match_query(qm: &QueryMatch, params: &[(String, String)]) -> bool {
        match qm.op {
            MatchOp::Exact => params
                .iter()
                .any(|(k, v)| k == &qm.key && v == &qm.value),
            MatchOp::Prefix => params
                .iter()
                .any(|(k, v)| k == &qm.key && v.starts_with(&qm.value)),
            MatchOp::Regex => params
                .iter()
                .any(|(k, v)| k == &qm.key && simple_regex_match(&qm.value, v)),
            MatchOp::NotEmpty => params
                .iter()
                .any(|(k, v)| k == &qm.key && !v.is_empty()),
        }
    }
}

impl Default for RouteMatcher {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl RouteLookup for RouteMatcher {
    async fn lookup_route(
        &self,
        protocol: ProtocolId,
        info: &RouteMatchInfo,
    ) -> Result<Option<RouteSnapshot>, ConrogateError> {
        Ok(self.match_route(protocol, info))
    }
}

/// 简易正则匹配（支持 * 通配符）
fn simple_regex_match(pattern: &str, text: &str) -> bool {
    if pattern == "*" || pattern == ".*" {
        return true;
    }

    // 如果包含 ^ 或 $，做简单的首尾匹配
    let pattern = pattern.trim_start_matches('^').trim_end_matches('$');

    // 支持 * 作为通配符
    if pattern.contains('*') {
        let parts: Vec<&str> = pattern.split('*').collect();
        if parts.is_empty() {
            return true;
        }

        let mut pos = 0;
        for (i, part) in parts.iter().enumerate() {
            if part.is_empty() {
                continue;
            }
            if i == 0 && !pattern.starts_with('*') {
                // 第一段必须从头匹配
                if !text[pos..].starts_with(part) {
                    return false;
                }
                pos += part.len();
            } else {
                match text[pos..].find(part) {
                    Some(idx) => pos += idx + part.len(),
                    None => return false,
                }
            }
        }

        // 如果 pattern 不以 * 结尾，则 text 必须在最后一段后结束
        if !pattern.ends_with('*') && pos != text.len() {
            return false;
        }
        return true;
    }

    // 无通配符 → 精确匹配
    text == pattern
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prefix_match() {
        let matcher = RouteMatcher::new();
        let conditions = RouteMatchConditions {
            path: PathMatch::Prefix("/api/".into()),
            ..Default::default()
        };
        let info = RouteMatchInfo {
            path: "/api/users".into(),
            method: Some("GET".into()),
            host: None,
            headers: vec![],
            query_params: vec![],
        };
        assert!(RouteMatcher::matches(&conditions, &info));
    }

    #[test]
    fn test_exact_path_match() {
        let conditions = RouteMatchConditions {
            path: PathMatch::Exact("/health".into()),
            ..Default::default()
        };
        let info_match = RouteMatchInfo {
            path: "/health".into(),
            method: None,
            host: None,
            headers: vec![],
            query_params: vec![],
        };
        assert!(RouteMatcher::matches(&conditions, &info_match));

        let info_no_match = RouteMatchInfo {
            path: "/healthz".into(),
            method: None,
            host: None,
            headers: vec![],
            query_params: vec![],
        };
        assert!(!RouteMatcher::matches(&conditions, &info_no_match));
    }

    #[test]
    fn test_method_match() {
        let conditions = RouteMatchConditions {
            path: PathMatch::Prefix("/".into()),
            methods: Some(vec!["GET".into(), "POST".into()]),
            ..Default::default()
        };
        let info_get = RouteMatchInfo {
            path: "/test".into(),
            method: Some("GET".into()),
            host: None,
            headers: vec![],
            query_params: vec![],
        };
        assert!(RouteMatcher::matches(&conditions, &info_get));

        let info_put = RouteMatchInfo {
            path: "/test".into(),
            method: Some("PUT".into()),
            host: None,
            headers: vec![],
            query_params: vec![],
        };
        assert!(!RouteMatcher::matches(&conditions, &info_put));
    }

    #[test]
    fn test_header_match() {
        let conditions = RouteMatchConditions {
            path: PathMatch::Prefix("/".into()),
            headers: vec![HeaderMatch {
                key: "X-Version".into(),
                op: MatchOp::Exact,
                value: "v2".into(),
            }],
            ..Default::default()
        };
        let info_match = RouteMatchInfo {
            path: "/test".into(),
            method: None,
            host: None,
            headers: vec![("x-version".into(), "v2".into())],
            query_params: vec![],
        };
        assert!(RouteMatcher::matches(&conditions, &info_match));

        let info_no_match = RouteMatchInfo {
            path: "/test".into(),
            method: None,
            host: None,
            headers: vec![("x-version".into(), "v1".into())],
            query_params: vec![],
        };
        assert!(!RouteMatcher::matches(&conditions, &info_no_match));
    }
}
