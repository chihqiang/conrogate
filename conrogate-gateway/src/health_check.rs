//! 主动健康检查器：定期 HTTP/TCP 探测上游节点。

use conrogate_contract::dto::UpstreamNodeDto;
use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// 主动健康检查器配置
#[derive(Clone)]
pub struct ActiveHealthCheckerConfig {
    /// 检查间隔
    pub interval: Duration,
    /// 连接超时
    pub connect_timeout: Duration,
    /// 健康判定：连续成功次数
    pub healthy_threshold: u32,
    /// 不健康判定：连续失败次数
    pub unhealthy_threshold: u32,
    /// HTTP 探测路径（None = TCP 探测）
    pub http_path: Option<String>,
}

impl Default for ActiveHealthCheckerConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(10),
            connect_timeout: Duration::from_secs(3),
            healthy_threshold: 2,
            unhealthy_threshold: 3,
            http_path: None,
        }
    }
}

/// 节点健康状态
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeHealth {
    /// 健康
    Healthy,
    /// 降级
    Degraded,
    /// 不可用
    Unhealthy,
}

/// 节点健康状态记录
struct NodeStatus {
    state: NodeHealth,
    consecutive_success: u32,
    consecutive_failure: u32,
    last_check: Instant,
}

/// 主动健康检查器
pub struct ActiveHealthChecker {
    config: ActiveHealthCheckerConfig,
    nodes: RwLock<HashMap<String, NodeStatus>>,
}

impl ActiveHealthChecker {
    pub fn new(config: ActiveHealthCheckerConfig) -> Self {
        Self {
            config,
            nodes: RwLock::new(HashMap::new()),
        }
    }

    /// 执行一次健康检查
    pub async fn check_node(&self, node: &UpstreamNodeDto) -> NodeHealth {
        let addr = &node.address;
        let success = if let Some(ref path) = self.config.http_path {
            self.http_probe(addr, path).await
        } else {
            self.tcp_probe(addr).await
        };

        let mut nodes = self.nodes.write().unwrap();
        let status = nodes.entry(addr.clone()).or_insert(NodeStatus {
            state: NodeHealth::Healthy,
            consecutive_success: 0,
            consecutive_failure: 0,
            last_check: Instant::now(),
        });

        status.last_check = Instant::now();

        if success {
            status.consecutive_success += 1;
            status.consecutive_failure = 0;
            if status.consecutive_success >= self.config.healthy_threshold {
                status.state = NodeHealth::Healthy;
            }
        } else {
            status.consecutive_failure += 1;
            status.consecutive_success = 0;
            if status.consecutive_failure >= self.config.unhealthy_threshold {
                status.state = NodeHealth::Unhealthy;
            } else if status.consecutive_failure > 0 {
                status.state = NodeHealth::Degraded;
            }
        }

        status.state.clone()
    }

    /// 获取节点健康状态
    pub fn get_health(&self, addr: &str) -> NodeHealth {
        let nodes = self.nodes.read().unwrap();
        nodes.get(addr).map(|s| s.state.clone()).unwrap_or(NodeHealth::Healthy)
    }

    /// 判断节点是否可调度
    pub fn is_schedulable(&self, addr: &str) -> bool {
        let nodes = self.nodes.read().unwrap();
        let status = nodes.get(addr);
        match status {
            Some(s) => s.state != NodeHealth::Unhealthy,
            None => true, // 未知状态默认可调度
        }
    }

    /// TCP 探测
    async fn tcp_probe(&self, addr: &str) -> bool {
        match tokio::time::timeout(
            self.config.connect_timeout,
            TcpStream::connect(addr),
        ).await {
            Ok(Ok(_)) => true,
            _ => false,
        }
    }

    /// HTTP 探测
    async fn http_probe(&self, addr: &str, path: &str) -> bool {
        let timeout_result: Result<Result<bool, std::io::Error>, _> = tokio::time::timeout(
            self.config.connect_timeout,
            async {
                let mut stream = TcpStream::connect(addr).await?;
                // 发送 HTTP GET 请求
                let request = format!(
                    "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
                    path, addr
                );
                stream.write_all(request.as_bytes()).await?;
                stream.flush().await?;

                // 读取响应状态行
                let mut buf = [0u8; 32];
                let n = stream.read(&mut buf).await?;
                if n == 0 {
                    return Ok(false);
                }

                // 检查 HTTP 状态码是否 2xx
                let response = String::from_utf8_lossy(&buf[..n]);
                Ok(response.starts_with("HTTP/1.1 2") || response.starts_with("HTTP/1.0 2"))
            },
        ).await;

        match timeout_result {
            Ok(Ok(success)) => success,
            _ => false,
        }
    }

    /// 启动定期检查后台任务
    pub fn spawn_periodic_check(
        self: std::sync::Arc<Self>,
        upstreams: std::sync::Arc<RwLock<HashMap<u64, (conrogate_contract::balancer::BalancerAlgorithm, Vec<UpstreamNodeDto>)>>>,
    ) {
        let config = self.config.clone();
        let checker = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(config.interval);
            interval.tick().await; // 跳过第一次立即触发
            loop {
                interval.tick().await;
                // 先 clone 出节点列表，释放锁后再做异步检查
                let nodes_to_check: Vec<UpstreamNodeDto> = {
                    let ups = upstreams.read().unwrap();
                    ups.values()
                        .flat_map(|(_, nodes)| nodes.iter().cloned())
                        .collect()
                };
                for node in nodes_to_check {
                    let _ = checker.check_node(&node).await;
                }
            }
        });
    }
}

impl Default for ActiveHealthChecker {
    fn default() -> Self {
        Self::new(ActiveHealthCheckerConfig::default())
    }
}
