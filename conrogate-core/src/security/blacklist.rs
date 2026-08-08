//! 全局 IP 黑名单匹配器（数据面内存态）。
//!
//! 与插件解耦的基础设施：拒绝的请求在路由匹配/插件执行前被拦截，
//! 对 HTTP / WebSocket / TCP 隧道三个协议统一生效。

use crate::contract::dto::IpBlacklistDto;
use chrono::{DateTime, Utc};
use ipnet::IpNet;
use std::net::IpAddr;
use std::sync::{Arc, RwLock};

/// 解析 IP 或 CIDR 为网段。裸 IP 视为 /32（IPv4）或 /128（IPv6）。
/// 注意：`IpNet` 的 FromStr 要求必须带前缀，裸 IP 需走 `IpAddr` 兜底。
pub fn parse_ip_or_cidr(s: &str) -> Option<IpNet> {
    if let Ok(net) = s.trim().parse::<IpNet>() {
        return Some(net);
    }
    s.trim().parse::<IpAddr>().ok().map(IpNet::from)
}

/// 黑名单条目：网段 + 过期时间（None=永久）
#[derive(Clone, Debug)]
struct Entry {
    net: IpNet,
    expires_at: Option<DateTime<Utc>>,
}

/// 数据面黑名单匹配器。
///
/// 读取路径无锁化（Arc 整体替换）：`is_blocked` 持读锁快速遍历；
/// 热载通过 `reload` 用新快照整体原子替换，不半套刷入。
#[derive(Default)]
pub struct BlacklistMatcher {
    entries: RwLock<Arc<Vec<Entry>>>,
}

impl BlacklistMatcher {
    pub fn new() -> Self {
        Self::default()
    }

    /// 原子替换黑名单（已过期的条目直接剔除，无需等下一次轮询）
    pub fn reload(&self, dtos: Vec<IpBlacklistDto>) {
        let now = Utc::now();
        let entries: Vec<Entry> = dtos
            .into_iter()
            .filter_map(|d| {
                let net = parse_ip_or_cidr(&d.ip_or_cidr)?;
                if d.expires_at.is_some_and(|exp| exp <= now) {
                    return None;
                }
                Some(Entry {
                    net,
                    expires_at: d.expires_at,
                })
            })
            .collect();
        tracing::info!(count = entries.len(), "ip blacklist reloaded");
        *self.entries.write().unwrap() = Arc::new(entries);
    }

    pub fn len(&self) -> usize {
        self.entries.read().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 判定 IP 是否命中黑名单（含过期过滤）。
    /// IP 解析失败视为未命中（上层已解析出 real_ip）。
    pub fn is_blocked(&self, ip: &str) -> bool {
        if self.is_empty() {
            return false;
        }
        let ip: IpAddr = match ip.trim().parse() {
            Ok(v) => v,
            Err(_) => return false,
        };
        let now = Utc::now();
        let entries = self.entries.read().unwrap();
        entries.iter().any(|e| {
            if e.expires_at.is_some_and(|exp| exp <= now) {
                return false;
            }
            e.net.contains(&ip)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(ip_or_cidr: &str, expires_in_secs: Option<i64>) -> IpBlacklistDto {
        IpBlacklistDto {
            id: 1,
            ip_or_cidr: ip_or_cidr.to_string(),
            reason: None,
            expires_at: expires_in_secs.map(|s| Utc::now() + chrono::Duration::seconds(s)),
            created_by: None,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn empty_matcher_never_blocks() {
        let m = BlacklistMatcher::new();
        assert!(m.is_empty());
        assert!(!m.is_blocked("1.2.3.4"));
    }

    #[test]
    fn matches_single_ip_and_cidr() {
        let m = BlacklistMatcher::new();
        m.reload(vec![entry("1.2.3.4", None), entry("10.0.0.0/24", None)]);
        assert!(m.is_blocked("1.2.3.4"));
        assert!(m.is_blocked("10.0.0.1"));
        assert!(m.is_blocked("10.0.0.255"));
        assert!(!m.is_blocked("1.2.3.5"));
        assert!(!m.is_blocked("10.0.1.1"));
        assert!(!m.is_blocked("::1"));
    }

    #[test]
    fn ipv6_cidr_matches() {
        let m = BlacklistMatcher::new();
        m.reload(vec![entry("2001:db8::/32", None)]);
        assert!(m.is_blocked("2001:db8::1"));
        assert!(!m.is_blocked("2001:db9::1"));
    }

    #[test]
    fn expired_entries_are_ignored() {
        let m = BlacklistMatcher::new();
        m.reload(vec![
            entry("1.2.3.4", Some(-10)),
            entry("2.3.4.5", Some(3600)),
        ]);
        assert!(!m.is_blocked("1.2.3.4"), "已过期条目不应拦截");
        assert!(m.is_blocked("2.3.4.5"));
    }

    #[test]
    fn invalid_cidr_entries_are_dropped_on_reload() {
        let m = BlacklistMatcher::new();
        m.reload(vec![entry("not-an-ip", None), entry("1.2.3.4", None)]);
        assert_eq!(m.len(), 1);
        assert!(m.is_blocked("1.2.3.4"));
    }

    #[test]
    fn invalid_ip_input_never_blocks() {
        let m = BlacklistMatcher::new();
        m.reload(vec![entry("1.2.3.4", None)]);
        assert!(!m.is_blocked("not-an-ip"));
    }

    #[test]
    fn reload_replaces_entries_atomically() {
        let m = BlacklistMatcher::new();
        m.reload(vec![entry("1.2.3.4", None)]);
        m.reload(vec![entry("5.6.7.8", None)]);
        assert!(!m.is_blocked("1.2.3.4"));
        assert!(m.is_blocked("5.6.7.8"));
    }
}
