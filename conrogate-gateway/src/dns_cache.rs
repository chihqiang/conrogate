//! DNS 解析缓存：首次调度前异步解析并缓存，按 DNS TTL 过期刷新。

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::RwLock;
use std::time::{Duration, Instant};

/// DNS 解析缓存条目
struct DnsCacheEntry {
    addrs: Vec<SocketAddr>,
    resolved_at: Instant,
    ttl: Duration,
}

/// DNS 解析缓存器
pub struct DnsResolver {
    cache: RwLock<HashMap<String, DnsCacheEntry>>,
    default_ttl: Duration,
}

impl DnsResolver {
    pub fn new() -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
            default_ttl: Duration::from_secs(60),
        }
    }

    /// 解析 host:port → SocketAddr 列表
    /// 优先从缓存读取，缓存过期则重新解析
    pub async fn resolve(&self, addr: &str) -> Result<Vec<SocketAddr>, std::io::Error> {
        // 检查缓存
        {
            let cache = self.cache.read().unwrap();
            if let Some(entry) = cache.get(addr) {
                if entry.resolved_at.elapsed() < entry.ttl {
                    return Ok(entry.addrs.clone());
                }
            }
        }

        // 解析地址
        let host_port: Vec<&str> = addr.rsplitn(2, ':').collect();
        if host_port.len() != 2 {
            // 如果不含端口，尝试整体解析
            let resolved = tokio::net::lookup_host(addr).await?;
            let addrs: Vec<SocketAddr> = resolved.collect();
            self.put_cache(addr.to_string(), addrs.clone());
            return Ok(addrs);
        }

        let port = host_port[0];
        let host = host_port[1];

        // 尝试直接解析为 SocketAddr（IP:port 格式不需要 DNS）
        if let Ok(socket_addr) = addr.parse::<SocketAddr>() {
            let addrs = vec![socket_addr];
            self.put_cache(addr.to_string(), addrs.clone());
            return Ok(addrs);
        }

        // DNS 解析
        let target = format!("{}:{}", host, port);
        let resolved = tokio::net::lookup_host(&target).await?;
        let addrs: Vec<SocketAddr> = resolved.collect();

        if addrs.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("DNS resolve failed: {}", addr),
            ));
        }

        self.put_cache(addr.to_string(), addrs.clone());
        Ok(addrs)
    }

    /// 写入缓存
    fn put_cache(&self, key: String, addrs: Vec<SocketAddr>) {
        let mut cache = self.cache.write().unwrap();
        cache.insert(key, DnsCacheEntry {
            addrs,
            resolved_at: Instant::now(),
            ttl: self.default_ttl,
        });
    }

    /// 清除缓存
    pub fn clear(&self) {
        let mut cache = self.cache.write().unwrap();
        cache.clear();
    }
}

impl Default for DnsResolver {
    fn default() -> Self {
        Self::new()
    }
}
