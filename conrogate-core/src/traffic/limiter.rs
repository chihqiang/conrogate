//! 限流器实现：固定窗口 / 滑动窗口 / 令牌桶（含 Redis 集群模式）。

use crate::contract::traffic::{LimitAlgorithm, Limiter};
use crate::contract::ConrogateError;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::sync::OnceCell;

/// 限流状态表条目上限，超过后触发过期清理（防止无界内存增长）
const MAX_ENTRIES: usize = 10_000;

// ── 固定窗口限流 ──

pub struct FixedWindowLimiter {
    windows: Mutex<HashMap<String, WindowEntry>>,
}

struct WindowEntry {
    count: u32,
    window_start: Instant,
}

impl FixedWindowLimiter {
    pub fn new() -> Self {
        Self {
            windows: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for FixedWindowLimiter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Limiter for FixedWindowLimiter {
    fn algorithm(&self) -> LimitAlgorithm {
        LimitAlgorithm::FixedWindow
    }

    async fn acquire(&self, key: &str, limit: u32, window: Duration) -> Result<(), ConrogateError> {
        let mut windows = self.windows.lock().unwrap();
        let now = Instant::now();

        // 防止无界增长：超过阈值时清理已过期的窗口
        if windows.len() >= MAX_ENTRIES {
            windows.retain(|_, e| now.duration_since(e.window_start) < window);
        }

        let entry = windows.entry(key.to_string()).or_insert(WindowEntry {
            count: 0,
            window_start: now,
        });

        // 窗口过期 → 重置
        if now.duration_since(entry.window_start) >= window {
            entry.count = 0;
            entry.window_start = now;
        }

        if entry.count >= limit {
            return Err(ConrogateError::RateLimited);
        }

        entry.count += 1;
        Ok(())
    }
}

// ── 滑动窗口限流 ──

pub struct SlidingWindowLimiter {
    windows: Mutex<HashMap<String, Vec<Instant>>>,
}

impl SlidingWindowLimiter {
    pub fn new() -> Self {
        Self {
            windows: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for SlidingWindowLimiter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Limiter for SlidingWindowLimiter {
    fn algorithm(&self) -> LimitAlgorithm {
        LimitAlgorithm::SlidingWindow
    }

    async fn acquire(&self, key: &str, limit: u32, window: Duration) -> Result<(), ConrogateError> {
        let mut windows = self.windows.lock().unwrap();
        let now = Instant::now();

        // 防止无界增长：超过阈值时清理所有记录均已过期（空闲）的键
        if windows.len() >= MAX_ENTRIES {
            let mut stale = Vec::new();
            for (k, v) in windows.iter() {
                let expired = v
                    .last()
                    .map(|&t| now.duration_since(t) >= window)
                    .unwrap_or(true);
                if expired {
                    stale.push(k.clone());
                }
            }
            for k in stale {
                windows.remove(&k);
            }
        }

        let timestamps = windows.entry(key.to_string()).or_default();

        // 清除窗口外的记录
        timestamps.retain(|t| now.duration_since(*t) < window);

        if timestamps.len() as u32 >= limit {
            return Err(ConrogateError::RateLimited);
        }

        timestamps.push(now);
        Ok(())
    }
}

// ── 令牌桶限流 ──

pub struct TokenBucketLimiter {
    buckets: Mutex<HashMap<String, TokenBucket>>,
    /// Redis 集群共享计数（可选）
    redis: Option<RedisClusterStore>,
}

struct TokenBucket {
    tokens: f64,
    capacity: f64,
    refill_rate: f64,
    last_refill: Instant,
}

/// Redis 集群共享令牌桶存储
pub struct RedisClusterStore {
    client: Option<redis::Client>,
    /// Lua 脚本：分布式令牌桶（基于服务器时间，避免跨节点时钟偏差）
    script: String,
    /// 复用连接管理器（避免每次请求新建连接）
    manager: OnceCell<redis::aio::ConnectionManager>,
}

impl RedisClusterStore {
    pub fn new(redis_url: &str) -> Self {
        Self {
            client: redis::Client::open(redis_url).ok(),
            script: r#"
-- 令牌桶：capacity 容量，refill_per_ms 每秒补充速率，基于服务器时间
local t = redis.call('TIME')
local now = t[1] * 1000 + t[2] / 1000
local capacity = tonumber(ARGV[1])
local refill_per_ms = tonumber(ARGV[2])

local bucket = redis.call('HMGET', KEYS[1], 'tokens', 'last')
local tokens = tonumber(bucket[1])
local last = tonumber(bucket[2])
if tokens == nil then tokens = capacity end
if last == nil then last = now end

tokens = tokens + (now - last) * refill_per_ms
if tokens > capacity then tokens = capacity end

if tokens < 1 then
    redis.call('HSET', KEYS[1], 'tokens', tokens, 'last', now)
    redis.call('PEXPIRE', KEYS[1], 1000)
    return 0
end

tokens = tokens - 1
redis.call('HSET', KEYS[1], 'tokens', tokens, 'last', now)
-- 空闲桶（已回满）自动过期，避免 Redis 中残留无界键
local ttl = math.ceil((capacity - tokens) / refill_per_ms)
redis.call('PEXPIRE', KEYS[1], ttl)
return 1
"#
            .to_string(),
            manager: OnceCell::new(),
        }
    }

    /// 获取复用的连接管理器（首次使用时懒加载；失败返回 None → fail-open）
    async fn conn(&self) -> Option<redis::aio::ConnectionManager> {
        let client = self.client.as_ref()?;
        if let Some(m) = self.manager.get() {
            return Some(m.clone());
        }
        let m = match redis::aio::ConnectionManager::new(client.clone()).await {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(error = %e, "rate limit redis connect failed, failing open");
                return None;
            }
        };
        let _ = self.manager.set(m.clone());
        Some(m)
    }

    /// 尝试从 Redis 获取令牌。
    /// Redis 不可用时 fail-open（放行），避免限流后端故障拖垮整个网关。
    pub async fn acquire(
        &self,
        key: &str,
        limit: u32,
        window: Duration,
    ) -> Result<(), ConrogateError> {
        let Some(mut conn) = self.conn().await else {
            tracing::warn!("rate limit redis unavailable, failing open");
            return Ok(());
        };
        let redis_key = format!("rl:{key}");
        let window_ms = window.as_millis().max(1) as i64;
        let refill_per_ms = limit as f64 / window_ms as f64;

        let result: Result<i32, _> = redis::cmd("EVAL")
            .arg(&self.script)
            .arg(1)
            .arg(&redis_key)
            .arg(limit)
            .arg(refill_per_ms)
            .query_async(&mut conn)
            .await;
        let result = match result {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = %e, "rate limit redis eval failed, failing open");
                return Ok(());
            }
        };
        if result == 1 {
            Ok(())
        } else {
            Err(ConrogateError::RateLimited)
        }
    }
}

impl TokenBucketLimiter {
    pub fn new() -> Self {
        Self {
            buckets: Mutex::new(HashMap::new()),
            redis: None,
        }
    }

    /// 启用 Redis 集群共享计数模式
    pub fn with_redis(mut self, redis_url: &str) -> Self {
        self.redis = Some(RedisClusterStore::new(redis_url));
        self
    }
}

impl Default for TokenBucketLimiter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Limiter for TokenBucketLimiter {
    fn algorithm(&self) -> LimitAlgorithm {
        LimitAlgorithm::TokenBucket
    }

    async fn acquire(&self, key: &str, limit: u32, window: Duration) -> Result<(), ConrogateError> {
        // 集群模式：使用 Redis 原子性 INCR + EXPIRE
        if let Some(ref redis_store) = self.redis {
            return redis_store.acquire(key, limit, window).await;
        }

        // 单机模式：进程内令牌桶
        let mut buckets = self.buckets.lock().unwrap();
        let now = Instant::now();

        // refill_rate = limit / window_secs（每秒补充的令牌数）
        let window_secs = window.as_secs_f64().max(0.001);
        let refill_rate = limit as f64 / window_secs;

        let entry = buckets.entry(key.to_string());
        // 是否本调用新建（新建桶即为满桶，无需清理）
        let is_fresh = matches!(entry, std::collections::hash_map::Entry::Vacant(_));
        let bucket = entry.or_insert(TokenBucket {
            tokens: limit as f64,
            capacity: limit as f64,
            refill_rate,
            last_refill: now,
        });

        // 补充令牌
        let elapsed = now.duration_since(bucket.last_refill).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * bucket.refill_rate).min(bucket.capacity);
        bucket.last_refill = now;

        // 空闲桶清理（非新建且已回满 = 长时间无请求）：移除，状态等价于新桶，
        // 防止每 IP/每 key 的桶永久驻留导致内存无限增长
        if !is_fresh && bucket.tokens >= bucket.capacity {
            buckets.remove(key);
            return Ok(());
        }

        if bucket.tokens < 1.0 {
            return Err(ConrogateError::RateLimited);
        }

        bucket.tokens -= 1.0;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fixed_window() {
        let limiter = FixedWindowLimiter::new();
        let window = Duration::from_secs(1);

        for _ in 0..3 {
            assert!(limiter.acquire("test", 3, window).await.is_ok());
        }
        assert!(limiter.acquire("test", 3, window).await.is_err());
    }

    #[tokio::test]
    async fn test_token_bucket() {
        let limiter = TokenBucketLimiter::new();
        let window = Duration::from_secs(1);

        for _ in 0..5 {
            assert!(limiter.acquire("test", 5, window).await.is_ok());
        }
        assert!(limiter.acquire("test", 5, window).await.is_err());
    }
}
