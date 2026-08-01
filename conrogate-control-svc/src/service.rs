//! 控制面业务服务层：聚合仓储 + 审计 + 配置版本管理。

use crate::audit::AuditService;
use conrogate_contract::dto::*;
use conrogate_contract::storage::*;
use conrogate_contract::ConrogateError;
use std::sync::Arc;

/// 控制面服务
pub struct ControlService {
    pub route_repo: Arc<dyn RouteRepo>,
    pub upstream_repo: Arc<dyn UpstreamRepo>,
    pub binding_repo: Arc<dyn PluginBindingRepo>,
    pub config_repo: Arc<dyn ConfigVersionRepo>,
    pub metric_repo: Arc<dyn MetricRepo>,
    pub event_repo: Arc<dyn EventRepo>,
    pub audit_repo: Arc<dyn AuditLogRepo>,
    pub node_app_repo: Arc<dyn NodeApplicationRepo>,
    pub plugin_repo: Arc<dyn InstalledPluginRepo>,
    pub audit: AuditService,
    /// 配置缓存（Redis 优先，可选）
    config_cache: Option<Arc<dyn ConfigCache>>,
}

impl ControlService {
    pub fn new(
        route_repo: Arc<dyn RouteRepo>,
        upstream_repo: Arc<dyn UpstreamRepo>,
        binding_repo: Arc<dyn PluginBindingRepo>,
        config_repo: Arc<dyn ConfigVersionRepo>,
        metric_repo: Arc<dyn MetricRepo>,
        event_repo: Arc<dyn EventRepo>,
        audit_repo: Arc<dyn AuditLogRepo>,
        node_app_repo: Arc<dyn NodeApplicationRepo>,
        plugin_repo: Arc<dyn InstalledPluginRepo>,
    ) -> Self {
        let audit = AuditService::new(audit_repo.clone());
        Self {
            route_repo,
            upstream_repo,
            binding_repo,
            config_repo,
            metric_repo,
            event_repo,
            audit_repo,
            node_app_repo,
            plugin_repo,
            audit,
            config_cache: None,
        }
    }

    /// 注入配置缓存（Redis 优先）；传入 None 可清除缓存
    pub fn with_config_cache(mut self, cache: Option<Arc<dyn ConfigCache>>) -> Self {
        self.config_cache = cache;
        self
    }

    // ── 路由管理 ──

    pub async fn create_route(&self, dto: CreateRouteDto, operator: Option<&str>) -> Result<RouteDto, ConrogateError> {
        let route = self.route_repo.create(dto).await?;
        self.audit.log(
            operator,
            "create",
            "route",
            Some(route.id),
            serde_json::to_value(&route).unwrap_or_default(),
            None,
        ).await;
        Ok(route)
    }

    pub async fn update_route(&self, dto: UpdateRouteDto, operator: Option<&str>) -> Result<RouteDto, ConrogateError> {
        let route = self.route_repo.update(dto).await?;
        self.audit.log(
            operator,
            "update",
            "route",
            Some(route.id),
            serde_json::to_value(&route).unwrap_or_default(),
            None,
        ).await;
        Ok(route)
    }

    pub async fn delete_route(&self, id: u64, operator: Option<&str>) -> Result<(), ConrogateError> {
        self.route_repo.soft_delete(id).await?;
        self.audit.log(
            operator,
            "delete",
            "route",
            Some(id),
            serde_json::json!({"id": id}),
            None,
        ).await;
        Ok(())
    }

    pub async fn get_route(&self, id: u64) -> Result<Option<RouteDto>, ConrogateError> {
        self.route_repo.find_by_id(id).await
    }

    pub async fn list_routes(&self, page: u32, page_size: u32) -> Result<PaginatedResult<RouteDto>, ConrogateError> {
        self.route_repo.list_paginated(page, page_size).await
    }

    // ── 上游管理 ──

    pub async fn create_upstream(&self, dto: CreateUpstreamDto, operator: Option<&str>) -> Result<UpstreamDto, ConrogateError> {
        let upstream = self.upstream_repo.create(dto).await?;
        self.audit.log(
            operator,
            "create",
            "upstream",
            Some(upstream.id),
            serde_json::to_value(&upstream).unwrap_or_default(),
            None,
        ).await;
        Ok(upstream)
    }

    pub async fn update_upstream(&self, dto: UpdateUpstreamDto, operator: Option<&str>) -> Result<UpstreamDto, ConrogateError> {
        let upstream = self.upstream_repo.update(dto).await?;
        self.audit.log(
            operator,
            "update",
            "upstream",
            Some(upstream.id),
            serde_json::to_value(&upstream).unwrap_or_default(),
            None,
        ).await;
        Ok(upstream)
    }

    pub async fn delete_upstream(&self, id: u64, operator: Option<&str>) -> Result<(), ConrogateError> {
        self.upstream_repo.soft_delete(id).await?;
        self.audit.log(
            operator,
            "delete",
            "upstream",
            Some(id),
            serde_json::json!({"id": id}),
            None,
        ).await;
        Ok(())
    }

    pub async fn get_upstream(&self, id: u64) -> Result<Option<UpstreamDto>, ConrogateError> {
        self.upstream_repo.find_by_id(id).await
    }

    pub async fn list_upstreams(&self, page: u32, page_size: u32) -> Result<PaginatedResult<UpstreamDto>, ConrogateError> {
        self.upstream_repo.list_paginated(page, page_size).await
    }

    // ── 插件绑定 ──

    pub async fn bind_plugin(&self, route_id: u64, dto: BindPluginDto, operator: Option<&str>) -> Result<PluginBindingDto, ConrogateError> {
        let binding = self.binding_repo.bind(route_id, dto).await?;
        self.audit.log(
            operator,
            "bind",
            "plugin_binding",
            Some(binding.id),
            serde_json::to_value(&binding).unwrap_or_default(),
            None,
        ).await;
        Ok(binding)
    }

    pub async fn unbind_plugin(&self, route_id: u64, plugin_name: &str, operator: Option<&str>) -> Result<(), ConrogateError> {
        self.binding_repo.unbind(route_id, plugin_name).await?;
        self.audit.log(
            operator,
            "unbind",
            "plugin_binding",
            None,
            serde_json::json!({"route_id": route_id, "plugin": plugin_name}),
            None,
        ).await;
        Ok(())
    }

    pub async fn list_plugin_bindings(&self, route_id: u64) -> Result<Vec<PluginBindingDto>, ConrogateError> {
        self.binding_repo.list_by_route(route_id).await
    }

    // ── 配置版本 ──

    pub async fn publish_config(
        &self,
        base_version: u64,
        operator: Option<&str>,
        remark: Option<&str>,
    ) -> Result<ConfigVersionDto, ConrogateError> {
        // 构建当前配置快照
        let routes = self.route_repo.list_enabled().await?;
        let upstreams = self.upstream_repo.list_all().await?;

        let mut bindings = Vec::new();
        for route in &routes {
            let route_bindings = self.binding_repo.list_by_route(route.id).await?;
            bindings.extend(route_bindings);
        }

        let snapshot = ConfigSnapshot {
            routes,
            upstreams,
            plugin_bindings: bindings,
        };

        let version = self.config_repo.publish(base_version, &snapshot, operator, remark).await?;

        // 写 Redis 配置缓存（失败不阻断发布，仅告警）
        if let Some(ref cache) = self.config_cache {
            if let Err(e) = cache.put_snapshot(version.version, &snapshot).await {
                tracing::warn!(
                    version = version.version,
                    error = %e,
                    "failed to write config snapshot to Redis cache"
                );
            }
        }

        self.audit.log(
            operator,
            "publish",
            "config_version",
            Some(version.version),
            serde_json::json!({"version": version.version, "base": version.base_version}),
            None,
        ).await;

        Ok(version)
    }

    pub async fn rollback_config(&self, target_version: u64, operator: Option<&str>) -> Result<ConfigVersionDto, ConrogateError> {
        let version = self.config_repo.rollback(target_version, operator).await?;

        // 写 Redis 配置缓存（失败不阻断回滚，仅告警）
        if let Some(ref cache) = self.config_cache {
            match self.config_repo.get_snapshot_by_version(version.version).await? {
                Some(snapshot) => {
                    if let Err(e) = cache.put_snapshot(version.version, &snapshot).await {
                        tracing::warn!(
                            version = version.version,
                            error = %e,
                            "failed to write rollback snapshot to Redis cache"
                        );
                    }
                }
                None => {
                    tracing::warn!(
                        version = version.version,
                        "snapshot not found for Redis cache after rollback"
                    );
                }
            }
        }

        self.audit.log(
            operator,
            "rollback",
            "config_version",
            Some(version.version),
            serde_json::json!({"target": target_version, "new_version": version.version}),
            None,
        ).await;

        Ok(version)
    }

    pub async fn list_config_versions(&self, page: u32, page_size: u32) -> Result<PaginatedResult<ConfigVersionDto>, ConrogateError> {
        self.config_repo.list_versions(page, page_size).await
    }

    pub async fn diff_config(&self, from: u64, to: u64) -> Result<ConfigDiff, ConrogateError> {
        self.config_repo.diff(from, to).await
    }

    // ── 指标查询 ──

    pub async fn query_metrics(&self, filter: MetricQuery) -> Result<Vec<MetricRow>, ConrogateError> {
        self.metric_repo.query(&filter).await
    }

    pub async fn overview_metrics(&self, range_min: u32) -> Result<OverviewMetric, ConrogateError> {
        self.metric_repo.overview(range_min).await
    }

    // ── 事件查询 ──

    pub async fn query_events(&self, filter: EventQuery, page: u32, page_size: u32) -> Result<PaginatedResult<EventRow>, ConrogateError> {
        self.event_repo.query(&filter, page, page_size).await
    }

    // ── 审计查询 ──

    pub async fn query_audit_logs(&self, filter: AuditLogQuery, page: u32, page_size: u32) -> Result<PaginatedResult<AuditLogRow>, ConrogateError> {
        self.audit_repo.query(&filter, page, page_size).await
    }

    // ── 节点管理 ──

    pub async fn list_nodes(&self) -> Result<Vec<NodeApplicationRow>, ConrogateError> {
        self.node_app_repo.list_all().await
    }

    // ── 插件管理 ──

    pub async fn list_plugins(&self, status: Option<conrogate_contract::plugin::PluginStatus>) -> Result<Vec<InstalledPluginDto>, ConrogateError> {
        self.plugin_repo.list(status).await
    }

    pub async fn receive_heartbeat(&self, heartbeat: Heartbeat) -> Result<(), ConrogateError> {
        self.node_app_repo.upsert(&heartbeat.gate_id, heartbeat.version).await
    }

    pub async fn receive_metrics(&self, batch: MetricsBatch) -> Result<(), ConrogateError> {
        self.metric_repo.upsert_batch(&batch.metrics).await
    }

    pub async fn receive_events(&self, batch: EventsBatch) -> Result<(), ConrogateError> {
        self.event_repo.insert_batch(&batch.events).await
    }
}
