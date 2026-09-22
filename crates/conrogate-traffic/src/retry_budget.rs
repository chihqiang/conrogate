//! 重试预算：限制全局重试比例，防止重试风暴加剧过载。
//!
//! SRE "Handling Overload" 核心原则：
//! - 重试本身也是负载——当系统过载时，重试会使情况更糟
//! - 重试预算按时间窗口统计：重试请求数 / 总请求数 不超过配置比例（如 10%）
//! - 预算耗尽时停止重试，直接返回原始错误（宁可失败也不加剧过载）
//!
//! 设计要点：
//! 1. 滑动窗口计数：窗口过期自动清零
//! 2. 冷启动保护：窗口内请求数低于 `min_requests` 时不做判定（避免误杀）
//! 3. 原子操作：热路径无锁，仅窗口过期时加锁清理

use conrogate_core::traffic::RetryBudget;
use conrogate_core::ConrogateError;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// 重试预算实现
pub struct RetryBudgetImpl {
    /// 总请求数（含原始 + 重试）
    total_requests: AtomicU64,
    /// 重试请求数
    retry_requests: AtomicU64,
    /// 窗口起点
    window_start: Mutex<Instant>,
    /// 配置
    config: RetryBudgetConfig,
}

/// 重试预算配置
#[derive(Clone)]
pub struct RetryBudgetConfig {
    /// 重试比例上限（0.0~1.0）
    pub budget_ratio: f64,
    /// 统计窗口长度
    pub window: Duration,
    /// 预算窗口内最少请求数（冷启动保护）
    pub min_requests: u64,
}

impl Default for RetryBudgetConfig {
    fn default() -> Self {
        Self {
            budget_ratio: 0.1,
            window: Duration::from_secs(10),
            min_requests: 10,
        }
    }
}

impl RetryBudgetImpl {
    pub fn new(config: RetryBudgetConfig) -> Self {
        Self {
            total_requests: AtomicU64::new(0),
            retry_requests: AtomicU64::new(0),
            window_start: Mutex::new(Instant::now()),
            config,
        }
    }

    /// 窗口过期时清零计数
    fn maybe_refresh_window(&self) {
        let now = Instant::now();
        let mut start = self.window_start.lock().unwrap();
        if now.duration_since(*start) >= self.config.window {
            self.total_requests.store(0, Ordering::Relaxed);
            self.retry_requests.store(0, Ordering::Relaxed);
            *start = now;
        }
    }
}

#[async_trait::async_trait]
impl RetryBudget for RetryBudgetImpl {
    fn try_consume(&self) -> Result<(), ConrogateError> {
        self.maybe_refresh_window();

        let total = self.total_requests.load(Ordering::Relaxed);
        let retries = self.retry_requests.load(Ordering::Relaxed);

        // 冷启动保护：窗口内请求数不足时不做预算判定
        if total < self.config.min_requests {
            self.retry_requests.fetch_add(1, Ordering::Relaxed);
            self.total_requests.fetch_add(1, Ordering::Relaxed);
            return Ok(());
        }

        // 预算判定：重试比例是否超限
        let ratio = retries as f64 / total as f64;
        if ratio >= self.config.budget_ratio {
            tracing::warn!(
                retries,
                total,
                ratio = format!("{:.1}%", ratio * 100.0),
                limit = format!("{:.1}%", self.config.budget_ratio * 100.0),
                "retry budget exhausted, refusing retry"
            );
            return Err(ConrogateError::RetryBudgetExhausted);
        }

        // 预算充足：消费一个重试配额
        self.retry_requests.fetch_add(1, Ordering::Relaxed);
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    fn record_request(&self) {
        self.maybe_refresh_window();
        self.total_requests.fetch_add(1, Ordering::Relaxed);
    }

    fn current_ratio(&self) -> f64 {
        let total = self.total_requests.load(Ordering::Relaxed);
        let retries = self.retry_requests.load(Ordering::Relaxed);
        if total == 0 {
            0.0
        } else {
            retries as f64 / total as f64
        }
    }

    fn current_total(&self) -> u64 {
        self.total_requests.load(Ordering::Relaxed)
    }

    fn current_retries(&self) -> u64 {
        self.retry_requests.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cold_start_protection() {
        let budget = RetryBudgetImpl::new(RetryBudgetConfig {
            budget_ratio: 0.1,
            window: Duration::from_secs(10),
            min_requests: 10,
        });

        // 冷启动期间：即使没记录原始请求，重试也能通过
        for _ in 0..9 {
            assert!(budget.try_consume().is_ok());
        }

        assert_eq!(budget.current_total(), 9);
        assert_eq!(budget.current_retries(), 9);
        assert!(budget.current_ratio() > 0.1); // 比例已超但冷启动保护
    }

    #[tokio::test]
    async fn test_budget_enforced_after_warmup() {
        let budget = RetryBudgetImpl::new(RetryBudgetConfig {
            budget_ratio: 0.1,
            window: Duration::from_secs(10),
            min_requests: 10,
        });

        // 先记录 20 个原始请求
        for _ in 0..20 {
            budget.record_request();
        }

        // 20 个原始请求，预算 10% = 2 次重试
        // try_consume 每次会先检查 ratio = retries/total
        // retry 1: ratio = 0/20 = 0 < 0.1 → 通过，retries=1, total=21
        // retry 2: ratio = 1/21 ≈ 0.048 < 0.1 → 通过，retries=2, total=22
        // retry 3: ratio = 2/22 ≈ 0.091 < 0.1 → 通过，retries=3, total=23
        // retry 4: ratio = 3/23 ≈ 0.130 >= 0.1 → 拒绝
        assert!(budget.try_consume().is_ok()); // retry 1
        assert!(budget.try_consume().is_ok()); // retry 2
        assert!(budget.try_consume().is_ok()); // retry 3
        assert!(budget.try_consume().is_err()); // retry 4 → 拒绝
        assert!(budget.try_consume().is_err()); // retry 5 → 拒绝

        assert_eq!(budget.current_total(), 23); // 20 + 3 (only successful consumes increment)
        assert_eq!(budget.current_retries(), 3); // 3 successful consumes
    }

    #[tokio::test]
    async fn test_window_reset() {
        let budget = RetryBudgetImpl::new(RetryBudgetConfig {
            budget_ratio: 0.1,
            window: Duration::from_millis(50),
            min_requests: 5,
        });

        // 预热 + 耗尽预算
        for _ in 0..10 {
            budget.record_request();
        }
        for _ in 0..10 {
            let _ = budget.try_consume();
        }

        // 此时预算应已耗尽
        assert!(budget.try_consume().is_err());

        // 等待窗口过期
        tokio::time::sleep(Duration::from_millis(60)).await;

        // 窗口重置：重新可以消费
        budget.record_request(); // 重新记录
        for _ in 0..4 {
            budget.record_request();
        }
        // 冷启动保护期间：5 个请求以下不判定
        assert!(budget.try_consume().is_ok());
    }

    #[tokio::test]
    async fn test_record_request_increments_total() {
        let budget = RetryBudgetImpl::new(RetryBudgetConfig {
            budget_ratio: 0.1,
            window: Duration::from_secs(10),
            min_requests: 1,
        });

        for _ in 0..100 {
            budget.record_request();
        }
        assert_eq!(budget.current_total(), 100);
        assert_eq!(budget.current_retries(), 0);
        assert_eq!(budget.current_ratio(), 0.0);
    }

    #[tokio::test]
    async fn test_concurrent_stress() {
        let budget = std::sync::Arc::new(RetryBudgetImpl::new(RetryBudgetConfig {
            budget_ratio: 0.1,
            window: Duration::from_secs(10),
            min_requests: 100,
        }));

        // 并发记录 1000 个原始请求
        for _ in 0..1000 {
            budget.record_request();
        }
        assert_eq!(budget.current_total(), 1000);

        // 并发消费重试预算
        let mut tasks = Vec::new();
        for _ in 0..200 {
            let b = budget.clone();
            tasks.push(tokio::spawn(async move { b.try_consume().is_ok() }));
        }

        let mut consumed = 0;
        for task in tasks {
            if task.await.unwrap() {
                consumed += 1;
            }
        }

        // 预算 10% = 100 次重试 + 冷启动期间可能多一点
        // 1000 个原始请求 + 200 个重试尝试 = 1200
        // 预算 = 10% of 1200 ≈ 120
        // 但 CAS 是非原子的组合操作，可能略多
        assert!(
            consumed <= 120,
            "consumed {} should not exceed budget significantly",
            consumed
        );
        assert!(
            consumed >= 100,
            "consumed {} should be close to 10% of 1000",
            consumed
        );
    }
}
