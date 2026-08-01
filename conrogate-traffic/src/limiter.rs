//! 限流器实现：固定窗口 / 滑动窗口 / 令牌桶。

use conrogate_contract::traffic::{LimitAlgorithm, Limiter};
use conrogate_contract::ConrogateError;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

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

        let timestamps = windows.entry(key.to_string()).or_insert(Vec::new());

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
}

struct TokenBucket {
    tokens: f64,
    capacity: f64,
    refill_rate: f64,
    last_refill: Instant,
}

impl TokenBucketLimiter {
    pub fn new() -> Self {
        Self {
            buckets: Mutex::new(HashMap::new()),
        }
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
        let mut buckets = self.buckets.lock().unwrap();
        let now = Instant::now();

        // refill_rate = limit / window_secs（每秒补充的令牌数）
        let window_secs = window.as_secs_f64().max(0.001);
        let refill_rate = limit as f64 / window_secs;

        let bucket = buckets.entry(key.to_string()).or_insert(TokenBucket {
            tokens: limit as f64,
            capacity: limit as f64,
            refill_rate,
            last_refill: now,
        });

        // 补充令牌
        let elapsed = now.duration_since(bucket.last_refill).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * bucket.refill_rate).min(bucket.capacity);
        bucket.last_refill = now;

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
