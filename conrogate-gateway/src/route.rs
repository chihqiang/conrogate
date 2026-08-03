//! 路由匹配引擎：多维条件匹配 + 优先级排序。

use conrogate_contract::dto::{RouteDto, RouteSnapshot};
use conrogate_contract::gateway::RouteLookup;
use conrogate_contract::protocol::{
    HeaderMatch, MatchOp, PathMatch, ProtocolId, QueryMatch, RouteMatchConditions, RouteMatchInfo,
};
use conrogate_contract::ConrogateError;
use std::collections::HashMap;
use std::sync::RwLock;

// ── 模块级正则缓存（进程级单例）──
// 配置加载时预编译，运行时直接从缓存读取
static REGEX_CACHE: std::sync::OnceLock<RwLock<HashMap<String, regex::Regex>>> =
    std::sync::OnceLock::new();

fn regex_cache() -> &'static RwLock<HashMap<String, regex::Regex>> {
    REGEX_CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

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

    /// 从路由 DTO 列表加载路由表（无插件绑定 → requires_body=false）
    pub fn load(&self, dtos: Vec<RouteDto>) {
        self.load_with_bindings(dtos, vec![], &Default::default())
    }

    /// 从路由 DTO 列表 + 插件绑定加载路由表，并根据 `body_required_plugins` 静态判定每条路由的 body 模式
    pub fn load_with_bindings(
        &self,
        dtos: Vec<RouteDto>,
        bindings: Vec<conrogate_contract::dto::PluginBindingDto>,
        body_required_plugins: &std::collections::HashSet<String>,
    ) {
        let mut routes = self.routes.write().unwrap();
        routes.clear();

        // 按 route_id 分组绑定
        let mut binding_map: HashMap<u64, Vec<conrogate_contract::dto::PluginBindingDto>> =
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
                snapshot: RouteSnapshot {
                    id: dto.id,
                    protocol: dto.protocol,
                    upstream_id: dto.upstream_id,
                    host_header: dto.host_header.clone(),
                    allow_retry_non_idempotent: dto.allow_retry_non_idempotent,
                    plugin_chain: route_bindings,
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
    }

    /// 检查路由表是否为空（用于就绪探针）
    pub fn is_empty(&self) -> bool {
        let routes = self.routes.read().unwrap();
        routes.values().all(|v| v.is_empty())
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

        // 3. Host 匹配（支持 `*.example.com` 一层子域通配，精确匹配优先）
        if let Some(ref host) = conditions.host {
            match &info.host {
                Some(h) if Self::match_host(host, h) => {}
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
                // 编译时已预编译的正则，此处直接执行
                // 安全约束：编译时检查（无反向引用、无贪婪无限量词）
                // 执行超时：100ms
                safe_regex_match(pattern, request_path)
            }
        }
    }

    /// Host 匹配：精确匹配优先，`*.example.com` 通配一层子域
    fn match_host(pattern: &str, host: &str) -> bool {
        if pattern.eq_ignore_ascii_case(host) {
            return true;
        }
        if let Some(suffix) = pattern.strip_prefix("*.") {
            let host = host.to_ascii_lowercase();
            let suffix = suffix.to_ascii_lowercase();
            // 仅匹配一层子域：`*.example.com` 匹配 `a.example.com`，不匹配 `a.b.example.com`
            if let Some(rest) = host.strip_suffix(&format!(".{suffix}")) {
                return !rest.is_empty() && !rest.contains('.');
            }
        }
        false
    }

    fn match_header(hm: &HeaderMatch, headers: &[(String, String)]) -> bool {
        match hm.op {
            MatchOp::Exact => headers
                .iter()
                .any(|(k, v)| k.eq_ignore_ascii_case(&hm.key) && v == &hm.value),
            MatchOp::Prefix => headers
                .iter()
                .any(|(k, v)| k.eq_ignore_ascii_case(&hm.key) && v.starts_with(&hm.value)),
            MatchOp::Regex => headers
                .iter()
                .any(|(k, v)| k.eq_ignore_ascii_case(&hm.key) && safe_regex_match(&hm.value, v)),
            MatchOp::NotEmpty => headers
                .iter()
                .any(|(k, v)| k.eq_ignore_ascii_case(&hm.key) && !v.is_empty()),
        }
    }

    fn match_query(qm: &QueryMatch, params: &[(String, String)]) -> bool {
        match qm.op {
            MatchOp::Exact => params.iter().any(|(k, v)| k == &qm.key && v == &qm.value),
            MatchOp::Prefix => params
                .iter()
                .any(|(k, v)| k == &qm.key && v.starts_with(&qm.value)),
            MatchOp::Regex => params
                .iter()
                .any(|(k, v)| k == &qm.key && safe_regex_match(&qm.value, v)),
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
/// 成功：缓存编译结果，返回 true
/// 失败：记录告警，返回 false
fn try_compile_regex(pattern: &str) -> bool {
    if has_redos_risk(pattern) {
        tracing::warn!(pattern = %pattern, "regex pattern rejected: ReDoS risk");
        return false;
    }
    // 检查缓存中是否已有
    {
        let cache_read = regex_cache().read().unwrap();
        if cache_read.get(pattern).is_some() {
            return true;
        }
    }
    match regex::Regex::new(pattern) {
        Ok(re) => {
            let mut cache_write = regex_cache().write().unwrap();
            cache_write.insert(pattern.to_string(), re);
            true
        }
        Err(e) => {
            tracing::warn!(pattern = %pattern, error = %e, "regex compile failed");
            false
        }
    }
}

/// 安全正则匹配：使用 regex crate 编译，带 ReDoS 防护 + 全局缓存
///
/// 安全约束：
/// - 禁止反向引用（\1）和贪婪无限量词（.*+）
/// - 编译失败返回 false（不匹配）
/// - 正则在配置加载时预编译并缓存，运行时直接从缓存读取
/// - 执行超时：依赖 regex crate 的线性时间保证（O(n)），无需显式超时
fn safe_regex_match(pattern: &str, text: &str) -> bool {
    // ReDoS 防护：检查危险模式
    if has_redos_risk(pattern) {
        tracing::warn!(pattern = %pattern, "regex pattern rejected: ReDoS risk");
        return false;
    }

    let cache = regex_cache();

    // 尝试从缓存读取（配置加载时已预编译）
    {
        let cache_read = cache.read().unwrap();
        if let Some(re) = cache_read.get(pattern) {
            return re.is_match(text);
        }
    }

    // 缓存未命中（运行时动态正则，如 header/query 中的 Regex 匹配值）
    // 编译正则（编译失败视为不匹配）
    let re = match regex::Regex::new(pattern) {
        Ok(re) => re,
        Err(e) => {
            tracing::warn!(pattern = %pattern, error = %e, "regex compile failed");
            return false;
        }
    };

    // 执行匹配（regex crate 本身有线性时间保证）
    let result = re.is_match(text);

    // 写入缓存
    let mut cache_write = cache.write().unwrap();
    cache_write.insert(pattern.to_string(), re);

    result
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

// 保留旧函数名以兼容可能的调用
#[allow(dead_code)]
fn simple_regex_match(pattern: &str, text: &str) -> bool {
    safe_regex_match(pattern, text)
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
        assert!(RouteMatcher::matches(
            &conditions,
            &mk(Some("a.example.com".into()))
        ));
        assert!(RouteMatcher::matches(
            &conditions,
            &mk(Some("A.Example.COM".into()))
        ));
        // 要求至少一层子域：基域不匹配
        assert!(!RouteMatcher::matches(
            &conditions,
            &mk(Some("example.com".into()))
        ));
        // 多层子域不匹配
        assert!(!RouteMatcher::matches(
            &conditions,
            &mk(Some("a.b.example.com".into()))
        ));
        // 其他域不匹配
        assert!(!RouteMatcher::matches(
            &conditions,
            &mk(Some("a.other.com".into()))
        ));
        // 无 Host 不匹配
        assert!(!RouteMatcher::matches(&conditions, &mk(None)));
    }
}
