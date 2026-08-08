//! 路由匹配引擎：多维条件匹配 + 优先级排序。

use crate::contract::dto::{RouteDto, RouteSnapshot};
use crate::contract::gateway::RouteLookup;
use crate::contract::protocol::{
    HeaderMatch, MatchOp, PathMatch, ProtocolId, QueryMatch, RouteMatchConditions, RouteMatchInfo,
};
use crate::contract::ConrogateError;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// 路由匹配引擎
///
/// 读取路径无锁化（Arc 整体替换）：`match_route`/`is_empty`/`needs_headers` 仅原子
/// clone 快照后无锁遍历；路由表热载通过 `load_with_bindings` 用新快照整体原子替换。
/// 与 `security::blacklist::BlacklistMatcher` 相同的读多写少模式。
pub struct RouteMatcher {
    // 按协议分组存储路由快照
    // Vec 按 priority 降序排列
    routes: RwLock<Arc<RouteTable>>,
}

/// 路由表快照：路由分组 + 每协议的 header 匹配条件存在性标志。
#[derive(Default)]
struct RouteTable {
    routes: HashMap<ProtocolId, Vec<RouteEntry>>,
    /// 对应 `routes` 中每个协议是否含 header 匹配条件（供调用方按需构造请求头信息）
    needs_headers: HashMap<ProtocolId, bool>,
}

struct RouteEntry {
    snapshot: RouteSnapshot,
    conditions: RouteMatchConditions,
    priority: i32,
    /// 正则已在配置加载时预编译，热路径直接执行，零锁零缓存查找
    compiled: CompiledMatchers,
}

/// 预编译匹配器：路径/Header/Query 正则均在配置加载时编译一次。
/// 相比运行时按模式字符串查全局缓存，省去每次匹配的 RwLock + 哈希查找。
struct CompiledMatchers {
    /// 路径正则（`PathMatch::Regex` 时存在）
    path_regex: Option<regex::Regex>,
    /// 与 `conditions.headers` 对齐：非 Regex op 为 None
    header_regexes: Vec<Option<regex::Regex>>,
    /// 与 `conditions.query_params` 对齐：非 Regex op 为 None
    query_regexes: Vec<Option<regex::Regex>>,
}

impl CompiledMatchers {
    fn compile(conditions: &RouteMatchConditions) -> Self {
        let path_regex = match &conditions.path {
            PathMatch::Regex(pattern) => compile_safe(pattern),
            _ => None,
        };
        let header_regexes = conditions
            .headers
            .iter()
            .map(|hm| {
                if hm.op == MatchOp::Regex {
                    compile_safe(&hm.value)
                } else {
                    None
                }
            })
            .collect();
        let query_regexes = conditions
            .query_params
            .iter()
            .map(|qm| {
                if qm.op == MatchOp::Regex {
                    compile_safe(&qm.value)
                } else {
                    None
                }
            })
            .collect();
        Self {
            path_regex,
            header_regexes,
            query_regexes,
        }
    }
}

impl RouteMatcher {
    pub fn new() -> Self {
        Self {
            routes: RwLock::new(Arc::new(RouteTable::default())),
        }
    }

    /// 从路由 DTO 列表加载路由表（无插件绑定 → requires_body=false）
    pub fn load(&self, dtos: Vec<RouteDto>) {
        self.load_with_bindings(dtos, vec![], &Default::default())
    }

    /// 从路由 DTO 列表 + 插件绑定加载路由表，并根据 `body_required_plugins` 静态判定每条路由的 body 模式
    pub fn load_with_bindings(
        &self,
        dtos: Vec<RouteDto>,
        bindings: Vec<crate::contract::dto::PluginBindingDto>,
        body_required_plugins: &std::collections::HashSet<String>,
    ) {
        // 本地构建新快照，最后整体原子替换（读取路径始终可见完整路由表）
        let mut routes: HashMap<ProtocolId, Vec<RouteEntry>> = HashMap::new();

        // 按 route_id 分组绑定
        let mut binding_map: HashMap<u64, Vec<crate::contract::dto::PluginBindingDto>> =
            HashMap::new();
        for b in bindings {
            if b.enabled {
                binding_map.entry(b.route_id).or_default().push(b);
            }
        }

        for dto in dtos {
            if !dto.enabled {
                continue;
            }

            // 正则预编译验证：编译失败的路由标记为 disabled 并告警
            if !validate_regex_patterns(&dto.match_conditions) {
                tracing::warn!(
                    route_id = dto.id,
                    "route disabled at load time: regex pattern compilation failed"
                );
                continue;
            }

            let route_bindings = binding_map.remove(&dto.id).unwrap_or_default();
            // 静态判定：该路由是否有 requires_body 插件
            let requires_body = route_bindings
                .iter()
                .any(|b| body_required_plugins.contains(&b.plugin_name));

            let entry = RouteEntry {
                conditions: dto.match_conditions.clone(),
                priority: dto.priority,
                compiled: CompiledMatchers::compile(&dto.match_conditions),
                snapshot: RouteSnapshot {
                    id: dto.id,
                    protocol: dto.protocol,
                    upstream_id: dto.upstream_id,
                    host_header: dto.host_header.clone().map(std::sync::Arc::from),
                    allow_retry_non_idempotent: dto.allow_retry_non_idempotent,
                    ws_strip_sensitive_headers: dto.ws_strip_sensitive_headers,
                    plugin_chain: std::sync::Arc::new(route_bindings),
                    requires_body,
                },
            };

            routes.entry(dto.protocol).or_default().push(entry);
        }

        // 每个协议组按 priority 降序排列，同 priority 时取 id 较小者
        for entries in routes.values_mut() {
            entries.sort_by(|a, b| {
                b.priority
                    .cmp(&a.priority)
                    .then(a.snapshot.id.cmp(&b.snapshot.id))
            });
        }

        let needs_headers = routes
            .iter()
            .map(|(&protocol, entries)| {
                (protocol, entries.iter().any(|e| !e.conditions.headers.is_empty()))
            })
            .collect();

        *self.routes.write().unwrap() = Arc::new(RouteTable {
            routes,
            needs_headers,
        });
    }

    /// 检查路由表是否为空（用于就绪探针）
    pub fn is_empty(&self) -> bool {
        let table = Arc::clone(&self.routes.read().unwrap());
        table.routes.values().all(|v| v.is_empty())
    }

    /// 指定协议的路由表是否含 header 匹配条件。
    /// 供调用方决定是否需要构造完整请求头信息（避免热路径无谓分配）。
    pub fn needs_headers(&self, protocol: ProtocolId) -> bool {
        let table = Arc::clone(&self.routes.read().unwrap());
        table.needs_headers.get(&protocol).copied().unwrap_or(false)
    }

    /// 匹配路由（快照无锁遍历）
    pub fn match_route(
        &self,
        protocol: ProtocolId,
        info: &RouteMatchInfo,
    ) -> Option<RouteSnapshot> {
        let table = Arc::clone(&self.routes.read().unwrap());
        let entries = table.routes.get(&protocol)?;

        for entry in entries {
            if Self::matches(&entry.conditions, &entry.compiled, info) {
                return Some(entry.snapshot.clone());
            }
        }
        None
    }

    /// HTTP 请求路由匹配：在**同一快照内**判定是否需要构造请求头、构造匹配信息并完成匹配。
    ///
    /// 调用方（HyperServiceBridge）此前先 `needs_headers()` 再 `match_route()` 需两次读锁，
    /// 两次之间若发生路由表热载替换，可能出现「表已含 header 条件但信息未构造头」的窗口
    /// 导致 header 路由漏匹配。此方法单次快照内原子完成判定 + 构造 + 匹配。
    pub fn match_http_request(
        &self,
        method: &http::Method,
        uri: &http::Uri,
        headers: &http::HeaderMap,
    ) -> (Option<RouteSnapshot>, RouteMatchInfo) {
        let table = Arc::clone(&self.routes.read().unwrap());
        let needs_headers = table
            .needs_headers
            .get(&ProtocolId::Http)
            .copied()
            .unwrap_or(false);
        let info = RouteMatchInfo::from_http_request(method, uri, headers, needs_headers);
        let matched = table
            .routes
            .get(&ProtocolId::Http)
            .and_then(|entries| {
                entries
                    .iter()
                    .find(|e| Self::matches(&e.conditions, &e.compiled, &info))
                    .map(|e| e.snapshot.clone())
            });
        (matched, info)
    }

    /// 检查单个路由条件是否匹配
    fn matches(
        conditions: &RouteMatchConditions,
        compiled: &CompiledMatchers,
        info: &RouteMatchInfo,
    ) -> bool {
        // 1. 路径匹配
        if !Self::match_path(&conditions.path, &info.path, compiled.path_regex.as_ref()) {
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

        // 3. Host 匹配（支持 `*.example.com` 一层子域通配，精确匹配优先）
        if let Some(ref host) = conditions.host {
            match &info.host {
                Some(h) if Self::match_host(host, h) => {}
                _ => return false,
            }
        }

        // 4. Header 匹配
        for (i, header_match) in conditions.headers.iter().enumerate() {
            if !Self::match_header(
                header_match,
                &info.headers,
                compiled.header_regexes.get(i).and_then(|r| r.as_ref()),
            ) {
                return false;
            }
        }

        // 5. Query 参数匹配
        for (i, query_match) in conditions.query_params.iter().enumerate() {
            if !Self::match_query(
                query_match,
                &info.query_params,
                compiled.query_regexes.get(i).and_then(|r| r.as_ref()),
            ) {
                return false;
            }
        }

        true
    }

    fn match_path(
        path_match: &PathMatch,
        request_path: &str,
        path_regex: Option<&regex::Regex>,
    ) -> bool {
        match path_match {
            PathMatch::Exact(p) => request_path == p,
            PathMatch::Prefix(p) => request_path.starts_with(p),
            // 配置加载时已预编译，此处直接执行（regex crate 保证线性时间）
            PathMatch::Regex(_) => path_regex.is_some_and(|re| re.is_match(request_path)),
        }
    }

    /// Host 匹配：精确匹配优先，`*.example.com` 通配一层子域
    /// （字节级大小写不敏感比较，零分配，避免热路径每次 format!/to_lowercase）
    fn match_host(pattern: &str, host: &str) -> bool {
        if pattern.eq_ignore_ascii_case(host) {
            return true;
        }
        if let Some(suffix) = pattern.strip_prefix("*.") {
            let host_bytes = host.as_bytes();
            let hlen = host_bytes.len();
            let slen = suffix.len();
            // 结构要求：`x.example.com`，其中 x 为单个标签（不含点）
            if hlen > slen + 1 && host_bytes[hlen - slen - 1] == b'.' {
                let label = &host[..hlen - slen - 1];
                if label.is_empty() || label.contains('.') {
                    return false;
                }
                return suffix.eq_ignore_ascii_case(&host[hlen - slen..]);
            }
        }
        false
    }

    fn match_header(
        hm: &HeaderMatch,
        headers: &[(String, String)],
        regex: Option<&regex::Regex>,
    ) -> bool {
        match hm.op {
            MatchOp::Exact => headers
                .iter()
                .any(|(k, v)| k.eq_ignore_ascii_case(&hm.key) && v == &hm.value),
            MatchOp::Prefix => headers
                .iter()
                .any(|(k, v)| k.eq_ignore_ascii_case(&hm.key) && v.starts_with(&hm.value)),
            MatchOp::Regex => headers.iter().any(|(k, v)| {
                k.eq_ignore_ascii_case(&hm.key) && regex.is_some_and(|re| re.is_match(v))
            }),
            MatchOp::NotEmpty => headers
                .iter()
                .any(|(k, v)| k.eq_ignore_ascii_case(&hm.key) && !v.is_empty()),
        }
    }

    fn match_query(
        qm: &QueryMatch,
        params: &[(String, String)],
        regex: Option<&regex::Regex>,
    ) -> bool {
        match qm.op {
            MatchOp::Exact => params.iter().any(|(k, v)| k == &qm.key && v == &qm.value),
            MatchOp::Prefix => params
                .iter()
                .any(|(k, v)| k == &qm.key && v.starts_with(&qm.value)),
            MatchOp::Regex => params.iter().any(|(k, v)| {
                k == &qm.key && regex.is_some_and(|re| re.is_match(v))
            }),
            MatchOp::NotEmpty => params.iter().any(|(k, v)| k == &qm.key && !v.is_empty()),
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

/// 预编译验证：检查路由条件中的所有正则模式是否可编译
/// 在配置加载时调用，编译失败的路由将被跳过（不加入路由表）
fn validate_regex_patterns(conditions: &RouteMatchConditions) -> bool {
    // 路径正则
    if let PathMatch::Regex(ref pattern) = conditions.path {
        if !try_compile_regex(pattern) {
            return false;
        }
    }
    // Header 正则
    for hm in &conditions.headers {
        if hm.op == MatchOp::Regex && !try_compile_regex(&hm.value) {
            return false;
        }
    }
    // Query 正则
    for qm in &conditions.query_params {
        if qm.op == MatchOp::Regex && !try_compile_regex(&qm.value) {
            return false;
        }
    }
    true
}

/// 尝试预编译正则（配置加载时调用）
/// 成功：返回 true；失败（含 ReDoS 风险）：记录告警，返回 false
fn try_compile_regex(pattern: &str) -> bool {
    compile_safe(pattern).is_some()
}

/// 安全编译正则：带 ReDoS 防护，编译结果直接由 `RouteEntry` 持有。
/// 安全约束：正则 crate 保证线性时间匹配（O(n)），运行时无需额外超时。
fn compile_safe(pattern: &str) -> Option<regex::Regex> {
    if has_redos_risk(pattern) {
        tracing::warn!(pattern = %pattern, "regex pattern rejected: ReDoS risk");
        return None;
    }
    match regex::Regex::new(pattern) {
        Ok(re) => Some(re),
        Err(e) => {
            tracing::warn!(pattern = %pattern, error = %e, "regex compile failed");
            None
        }
    }
}

/// 检查正则是否有 ReDoS 风险
/// 禁止：反向引用（\1）、贪婪无限量词（.*+）
fn has_redos_risk(pattern: &str) -> bool {
    // 检查反向引用 \1 \2 等
    let bytes = pattern.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit() {
            return true;
        }
        i += 1;
    }

    // 检查贪婪无限量词：*+ 或 + 的嵌套
    // 简易检查：(.+)+ 或 (.*)* 模式
    if pattern.contains("(.+") && pattern.contains("+") {
        // 检查嵌套量词
        if pattern.contains("(.+)+")
            || pattern.contains("(.*)*")
            || pattern.contains("(.+)*")
            || pattern.contains("(.*)+")
        {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prefix_match() {
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
        assert!(RouteMatcher::matches(&conditions, &CompiledMatchers::compile(&conditions), &info));
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
        assert!(RouteMatcher::matches(&conditions, &CompiledMatchers::compile(&conditions), &info_match));

        let info_no_match = RouteMatchInfo {
            path: "/healthz".into(),
            method: None,
            host: None,
            headers: vec![],
            query_params: vec![],
        };
        assert!(!RouteMatcher::matches(&conditions, &CompiledMatchers::compile(&conditions), &info_no_match));
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
        assert!(RouteMatcher::matches(&conditions, &CompiledMatchers::compile(&conditions), &info_get));

        let info_put = RouteMatchInfo {
            path: "/test".into(),
            method: Some("PUT".into()),
            host: None,
            headers: vec![],
            query_params: vec![],
        };
        assert!(!RouteMatcher::matches(&conditions, &CompiledMatchers::compile(&conditions), &info_put));
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
        assert!(RouteMatcher::matches(&conditions, &CompiledMatchers::compile(&conditions), &info_match));

        let info_no_match = RouteMatchInfo {
            path: "/test".into(),
            method: None,
            host: None,
            headers: vec![("x-version".into(), "v1".into())],
            query_params: vec![],
        };
        assert!(!RouteMatcher::matches(&conditions, &CompiledMatchers::compile(&conditions), &info_no_match));
    }

    #[test]
    fn test_needs_headers_tracks_header_conditions() {
        let m = RouteMatcher::new();
        assert!(!m.needs_headers(ProtocolId::Http), "空表无需解析请求头");

        // 无 header 条件的路由：needs_headers 保持 false
        m.load(vec![RouteDto {
            id: 1,
            name: "plain".into(),
            protocol: ProtocolId::Http,
            match_conditions: RouteMatchConditions {
                path: PathMatch::Prefix("/".into()),
                ..Default::default()
            },
            priority: 1,
            upstream_id: Some(1),
            host_header: None,
            allow_retry_non_idempotent: false,
            ws_strip_sensitive_headers: false,
            enabled: true,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }]);
        assert!(!m.needs_headers(ProtocolId::Http), "无 header 条件不应请求头");

        // 含 header 条件后：needs_headers 为 true，且 lite 信息（空 headers）不误匹配
        m.load(vec![RouteDto {
            id: 2,
            name: "header-route".into(),
            protocol: ProtocolId::Http,
            match_conditions: RouteMatchConditions {
                path: PathMatch::Prefix("/".into()),
                headers: vec![HeaderMatch {
                    key: "X-Version".into(),
                    op: MatchOp::Exact,
                    value: "v2".into(),
                }],
                ..Default::default()
            },
            priority: 1,
            upstream_id: Some(1),
            host_header: None,
            allow_retry_non_idempotent: false,
            ws_strip_sensitive_headers: false,
            enabled: true,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }]);
        assert!(m.needs_headers(ProtocolId::Http));

        // 原子匹配：同一快照内判定 + 构造 + 匹配，header 条件存在时信息携带完整头
        let req = http::Request::builder()
            .method("GET")
            .uri("/api")
            .header("X-Version", "v2")
            .body(())
            .unwrap();
        let (matched, info) =
            m.match_http_request(req.method(), req.uri(), req.headers());
        assert!(matched.is_some(), "header 条件路由应命中");
        assert_eq!(
            info.headers,
            vec![("x-version".to_string(), "v2".to_string())],
            "含 header 条件时应构造完整头信息"
        );
    }

    #[test]
    fn test_host_wildcard_match() {
        // `*.example.com` 匹配一层子域
        let conditions = RouteMatchConditions {
            path: PathMatch::Prefix("/".into()),
            host: Some("*.example.com".into()),
            ..Default::default()
        };
        let mk = |host: Option<String>| RouteMatchInfo {
            path: "/api".into(),
            method: None,
            host,
            headers: vec![],
            query_params: vec![],
        };
        let compiled = CompiledMatchers::compile(&conditions);
        assert!(RouteMatcher::matches(
            &conditions,
            &compiled,
            &mk(Some("a.example.com".into()))
        ));
        assert!(RouteMatcher::matches(
            &conditions,
            &compiled,
            &mk(Some("A.Example.COM".into()))
        ));
        // 要求至少一层子域：基域不匹配
        assert!(!RouteMatcher::matches(
            &conditions,
            &compiled,
            &mk(Some("example.com".into()))
        ));
        // 多层子域不匹配
        assert!(!RouteMatcher::matches(
            &conditions,
            &compiled,
            &mk(Some("a.b.example.com".into()))
        ));
        // 其他域不匹配
        assert!(!RouteMatcher::matches(
            &conditions,
            &compiled,
            &mk(Some("a.other.com".into()))
        ));
        // 无 Host 不匹配
        assert!(!RouteMatcher::matches(&conditions, &compiled, &mk(None)));
    }
}
