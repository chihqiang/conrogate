//! 仓储层 Trait 定义。

use crate::contract::dto::*;
use crate::contract::error::ConrogateError;
use async_trait::async_trait;

// ── 路由仓储 ──

#[async_trait]
pub trait ReadOnlyRouteRepo: Send + Sync {
    async fn list_enabled(&self) -> Result<Vec<RouteDto>, ConrogateError>;
    async fn find_by_id(&self, id: u64) -> Result<Option<RouteDto>, ConrogateError>;
}

#[async_trait]
pub trait RouteRepo: ReadOnlyRouteRepo {
    async fn create(&self, dto: CreateRouteDto) -> Result<RouteDto, ConrogateError>;
    async fn update(&self, dto: UpdateRouteDto) -> Result<RouteDto, ConrogateError>;
    async fn soft_delete(&self, id: u64) -> Result<(), ConrogateError>;
    async fn list_paginated(
        &self,
        page: u32,
        page_size: u32,
    ) -> Result<PaginatedResult<RouteDto>, ConrogateError>;
}

// ── 上游仓储 ──

#[async_trait]
pub trait ReadOnlyUpstreamRepo: Send + Sync {
    async fn list_all(&self) -> Result<Vec<UpstreamDto>, ConrogateError>;
    async fn find_by_id(&self, id: u64) -> Result<Option<UpstreamDto>, ConrogateError>;
    async fn find_by_route(&self, route_id: u64) -> Result<Option<UpstreamDto>, ConrogateError>;
}

#[async_trait]
pub trait UpstreamRepo: ReadOnlyUpstreamRepo {
    async fn create(&self, dto: CreateUpstreamDto) -> Result<UpstreamDto, ConrogateError>;
    async fn update(&self, dto: UpdateUpstreamDto) -> Result<UpstreamDto, ConrogateError>;
    async fn soft_delete(&self, id: u64) -> Result<(), ConrogateError>;
    async fn list_paginated(
        &self,
        page: u32,
        page_size: u32,
    ) -> Result<PaginatedResult<UpstreamDto>, ConrogateError>;
}

// ── 插件绑定仓储 ──

#[async_trait]
pub trait ReadOnlyPluginBindingRepo: Send + Sync {
    async fn list_by_route(&self, route_id: u64) -> Result<Vec<PluginBindingDto>, ConrogateError>;
}

#[async_trait]
pub trait PluginBindingRepo: ReadOnlyPluginBindingRepo {
    async fn bind(
        &self,
        route_id: u64,
        dto: BindPluginDto,
    ) -> Result<PluginBindingDto, ConrogateError>;
    async fn update(
        &self,
        route_id: u64,
        plugin_name: &str,
        dto: UpdatePluginBindingDto,
    ) -> Result<PluginBindingDto, ConrogateError>;
    async fn unbind(&self, route_id: u64, plugin_name: &str) -> Result<(), ConrogateError>;
}

// ── 配置版本仓储 ──

#[async_trait]
pub trait ConfigVersionRepo: Send + Sync {
    async fn publish(
        &self,
        base_version: u64,
        snapshot: &ConfigSnapshot,
        created_by: Option<&str>,
        remark: Option<&str>,
    ) -> Result<ConfigVersionDto, ConrogateError>;

    async fn list_versions(
        &self,
        page: u32,
        page_size: u32,
    ) -> Result<PaginatedResult<ConfigVersionDto>, ConrogateError>;

    async fn find_by_version(
        &self,
        version: u64,
    ) -> Result<Option<ConfigVersionDto>, ConrogateError>;
    async fn latest_version(&self) -> Result<Option<ConfigVersionDto>, ConrogateError>;

    /// 按版本号获取配置快照内容（用于回滚后写 Redis 缓存）
    async fn get_snapshot_by_version(
        &self,
        version: u64,
    ) -> Result<Option<ConfigSnapshot>, ConrogateError>;

    /// 将快照回写到业务表（routes / upstreams / route_plugin_bindings）。
    ///
    /// gate 配置热加载直接读业务表，回滚只有在回写业务表后才会生效：
    /// 按名称 upsert 快照中的路由/上游（含节点替换与路由→上游引用重映射），
    /// 软删除快照中不存在的活跃路由与上游，并重新对齐插件绑定。
    async fn apply_snapshot(&self, snapshot: &ConfigSnapshot) -> Result<(), ConrogateError>;

    async fn rollback(
        &self,
        target_version: u64,
        created_by: Option<&str>,
    ) -> Result<ConfigVersionDto, ConrogateError>;

    async fn diff(&self, from: u64, to: u64) -> Result<ConfigDiff, ConrogateError>;
}

// ── 指标仓储 ──

#[async_trait]
pub trait MetricRepo: Send + Sync {
    async fn upsert_batch(&self, metrics: &[MetricRow]) -> Result<(), ConrogateError>;
    async fn query(&self, filter: &MetricQuery) -> Result<Vec<MetricRow>, ConrogateError>;
    async fn overview(&self, range_min: u32) -> Result<OverviewMetric, ConrogateError>;
}

// ── 事件仓储 ──

#[async_trait]
pub trait EventRepo: Send + Sync {
    async fn insert_batch(&self, events: &[EventRow]) -> Result<(), ConrogateError>;
    async fn query(
        &self,
        filter: &EventQuery,
        page: u32,
        page_size: u32,
    ) -> Result<PaginatedResult<EventRow>, ConrogateError>;
}

// ── 审计日志仓储 ──

#[async_trait]
pub trait AuditLogRepo: Send + Sync {
    async fn insert(&self, row: &AuditLogRow) -> Result<(), ConrogateError>;
    async fn query(
        &self,
        filter: &AuditLogQuery,
        page: u32,
        page_size: u32,
    ) -> Result<PaginatedResult<AuditLogRow>, ConrogateError>;
}

// ── 节点应用记录仓储 ──

#[async_trait]
pub trait NodeApplicationRepo: Send + Sync {
    /// upsert 节点心跳：更新版本号与 last_seen（最近心跳时间）
    async fn upsert(
        &self,
        gate_id: &str,
        version: u64,
        last_seen: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), ConrogateError>;
    async fn count_by_version(&self, version: u64) -> Result<u32, ConrogateError>;
    async fn list_all(&self) -> Result<Vec<NodeApplicationRow>, ConrogateError>;
    async fn list_stale(
        &self,
        before: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<NodeApplicationRow>, ConrogateError>;
}

// ── 已安装插件仓储 ──

#[async_trait]
pub trait InstalledPluginRepo: Send + Sync {
    async fn list(
        &self,
        status: Option<crate::contract::plugin::PluginStatus>,
    ) -> Result<Vec<InstalledPluginDto>, ConrogateError>;
    async fn find_by_name(&self, name: &str) -> Result<Option<InstalledPluginDto>, ConrogateError>;
    async fn insert(&self, dto: &InstalledPluginDto) -> Result<(), ConrogateError>;
    async fn update_status(
        &self,
        name: &str,
        status: crate::contract::plugin::PluginStatus,
    ) -> Result<(), ConrogateError>;
    async fn soft_delete(&self, name: &str) -> Result<(), ConrogateError>;
}

// ── 全局 IP 黑名单仓储 ──

#[async_trait]
pub trait IpBlacklistRepo: Send + Sync {
    /// 全量拉取（含已过期，由调用方过滤），供数据面热载使用
    async fn list_all(&self) -> Result<Vec<IpBlacklistDto>, ConrogateError>;
    /// 分页查询（keyword 可选，模糊匹配 ip_or_cidr）
    async fn list_paginated(
        &self,
        filter: &IpBlacklistQuery,
        page: u32,
        page_size: u32,
    ) -> Result<PaginatedResult<IpBlacklistDto>, ConrogateError>;
    /// 拉黑：ip_or_cidr 已存在时幂等更新（刷新原因/过期时间）
    async fn upsert(&self, dto: &CreateIpBlacklistDto) -> Result<IpBlacklistDto, ConrogateError>;
    /// 解除拉黑；条目不存在时返回 NotFound
    async fn delete(&self, id: u64) -> Result<(), ConrogateError>;
}

// ── 配置缓存中间件 ──

#[async_trait]
pub trait ConfigCache: Send + Sync {
    async fn get_version(&self) -> Result<Option<u64>, ConrogateError>;
    async fn get_snapshot(&self) -> Result<Option<ConfigSnapshot>, ConrogateError>;
    async fn put_snapshot(
        &self,
        version: u64,
        snapshot: &ConfigSnapshot,
    ) -> Result<(), ConrogateError>;
    /// 使缓存的配置快照失效（发布失败时调用，数据面降级直连 DB 轮询，
    /// 避免读到过期版本造成长时间不一致）
    async fn invalidate(&self) -> Result<(), ConrogateError>;
    async fn subscribe_changes(
        &self,
    ) -> Result<Option<tokio::sync::watch::Receiver<u64>>, ConrogateError>;
}

// ── 配置快照加载器 ──

#[async_trait]
pub trait ConfigLoader: Send + Sync {
    async fn load_snapshot(&self) -> Result<ConfigSnapshot, ConrogateError>;
    async fn current_version(&self) -> Result<u64, ConrogateError>;
}
