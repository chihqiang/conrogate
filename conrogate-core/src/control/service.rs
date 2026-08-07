//! 控制面业务服务层：聚合仓储 + 审计 + 配置版本管理。

use super::audit::AuditService;
use crate::contract::dto::*;
use crate::contract::storage::*;
use crate::contract::ConrogateError;
use std::sync::Arc;

/// 单批上报条数上限（批量大小上限 1000 条/批）
const REPORT_BATCH_LIMIT: usize = 1000;

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
    pub ip_blacklist_repo: Arc<dyn IpBlacklistRepo>,
    pub audit: AuditService,
    /// 配置缓存（Redis 优先，可选）
    config_cache: Option<Arc<dyn ConfigCache>>,
}

impl ControlService {
    #[allow(clippy::too_many_arguments)]
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
        ip_blacklist_repo: Arc<dyn IpBlacklistRepo>,
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
            ip_blacklist_repo,
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

    pub async fn create_route(
        &self,
        dto: CreateRouteDto,
        operator: Option<&str>,
        trace_id: &str,
    ) -> Result<RouteDto, ConrogateError> {
        let route = self.route_repo.create(dto).await?;
        self.audit
            .log(
                operator,
                "create",
                "route",
                Some(route.id),
                serde_json::to_value(&route).unwrap_or_default(),
                Some(trace_id.to_string()),
            )
            .await;
        Ok(route)
    }

    pub async fn update_route(
        &self,
        dto: UpdateRouteDto,
        operator: Option<&str>,
        trace_id: &str,
    ) -> Result<RouteDto, ConrogateError> {
        let route = self.route_repo.update(dto).await?;
        self.audit
            .log(
                operator,
                "update",
                "route",
                Some(route.id),
                serde_json::to_value(&route).unwrap_or_default(),
                Some(trace_id.to_string()),
            )
            .await;
        Ok(route)
    }

    pub async fn delete_route(
        &self,
        id: u64,
        operator: Option<&str>,
        trace_id: &str,
    ) -> Result<(), ConrogateError> {
        self.route_repo.soft_delete(id).await?;
        self.audit
            .log(
                operator,
                "delete",
                "route",
                Some(id),
                serde_json::json!({"id": id}),
                Some(trace_id.to_string()),
            )
            .await;
        Ok(())
    }

    pub async fn get_route(&self, id: u64) -> Result<Option<RouteDto>, ConrogateError> {
        self.route_repo.find_by_id(id).await
    }

    pub async fn list_routes(
        &self,
        page: u32,
        page_size: u32,
    ) -> Result<PaginatedResult<RouteDto>, ConrogateError> {
        self.route_repo.list_paginated(page, page_size).await
    }

    // ── 上游管理 ──

    pub async fn create_upstream(
        &self,
        dto: CreateUpstreamDto,
        operator: Option<&str>,
        trace_id: &str,
    ) -> Result<UpstreamDto, ConrogateError> {
        let upstream = self.upstream_repo.create(dto).await?;
        self.audit
            .log(
                operator,
                "create",
                "upstream",
                Some(upstream.id),
                serde_json::to_value(&upstream).unwrap_or_default(),
                Some(trace_id.to_string()),
            )
            .await;
        Ok(upstream)
    }

    pub async fn update_upstream(
        &self,
        dto: UpdateUpstreamDto,
        operator: Option<&str>,
        trace_id: &str,
    ) -> Result<UpstreamDto, ConrogateError> {
        let upstream = self.upstream_repo.update(dto).await?;
        self.audit
            .log(
                operator,
                "update",
                "upstream",
                Some(upstream.id),
                serde_json::to_value(&upstream).unwrap_or_default(),
                Some(trace_id.to_string()),
            )
            .await;
        Ok(upstream)
    }

    pub async fn delete_upstream(
        &self,
        id: u64,
        operator: Option<&str>,
        trace_id: &str,
    ) -> Result<(), ConrogateError> {
        // 删除保护：被路由绑定的上游禁止删除，需先解绑相关路由
        let bindings = self.upstream_repo.list_route_bindings(id).await?;
        if !bindings.is_empty() {
            let names: Vec<&str> = bindings.iter().map(|b| b.name.as_str()).collect();
            return Err(ConrogateError::Conflict(format!(
                "upstream is bound by {} route(s): {}; unbind first before deleting",
                bindings.len(),
                names.join(", ")
            )));
        }
        self.upstream_repo.soft_delete(id).await?;
        self.audit
            .log(
                operator,
                "delete",
                "upstream",
                Some(id),
                serde_json::json!({"id": id}),
                Some(trace_id.to_string()),
            )
            .await;
        Ok(())
    }

    /// 返回引用该上游的活跃路由（删除弹窗展示绑定关系）
    pub async fn list_upstream_route_bindings(
        &self,
        id: u64,
    ) -> Result<Vec<UpstreamRouteBindingDto>, ConrogateError> {
        self.upstream_repo.list_route_bindings(id).await
    }

    pub async fn get_upstream(&self, id: u64) -> Result<Option<UpstreamDto>, ConrogateError> {
        self.upstream_repo.find_by_id(id).await
    }

    pub async fn list_upstreams(
        &self,
        page: u32,
        page_size: u32,
    ) -> Result<PaginatedResult<UpstreamDto>, ConrogateError> {
        self.upstream_repo.list_paginated(page, page_size).await
    }

    // ── 插件绑定 ──

    pub async fn bind_plugin(
        &self,
        route_id: u64,
        dto: BindPluginDto,
        operator: Option<&str>,
        trace_id: &str,
    ) -> Result<PluginBindingDto, ConrogateError> {
        let binding = self.binding_repo.bind(route_id, dto).await?;
        self.audit
            .log(
                operator,
                "bind",
                "plugin_binding",
                Some(binding.id),
                serde_json::to_value(&binding).unwrap_or_default(),
                Some(trace_id.to_string()),
            )
            .await;
        Ok(binding)
    }

    pub async fn unbind_plugin(
        &self,
        route_id: u64,
        plugin_name: &str,
        operator: Option<&str>,
        trace_id: &str,
    ) -> Result<(), ConrogateError> {
        self.binding_repo.unbind(route_id, plugin_name).await?;
        self.audit
            .log(
                operator,
                "unbind",
                "plugin_binding",
                None,
                serde_json::json!({"route_id": route_id, "plugin": plugin_name}),
                Some(trace_id.to_string()),
            )
            .await;
        Ok(())
    }

    pub async fn list_plugin_bindings(
        &self,
        route_id: u64,
    ) -> Result<Vec<PluginBindingDto>, ConrogateError> {
        self.binding_repo.list_by_route(route_id).await
    }

    pub async fn update_plugin_binding(
        &self,
        route_id: u64,
        plugin_name: &str,
        dto: UpdatePluginBindingDto,
        operator: Option<&str>,
        trace_id: &str,
    ) -> Result<PluginBindingDto, ConrogateError> {
        let binding = self.binding_repo.update(route_id, plugin_name, dto).await?;
        self.audit
            .log(
                operator,
                "update",
                "plugin_binding",
                Some(binding.id),
                serde_json::to_value(&binding).unwrap_or_default(),
                Some(trace_id.to_string()),
            )
            .await;
        Ok(binding)
    }

    // ── 配置版本 ──

    pub async fn publish_config(
        &self,
        base_version: u64,
        operator: Option<&str>,
        trace_id: &str,
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

        let version = self
            .config_repo
            .publish(base_version, &snapshot, operator, remark)
            .await?;

        // 写 Redis 配置缓存（失败不阻断发布：使缓存失效并告警，
        // 数据面降级直连 DB 轮询，避免读到过期快照造成长时间不一致）
        if let Some(ref cache) = self.config_cache {
            if let Err(e) = cache.put_snapshot(version.version, &snapshot).await {
                tracing::error!(
                    version = version.version,
                    error = %e,
                    "config snapshot Redis cache write failed after retries, invalidating cache"
                );
                if let Err(ie) = cache.invalidate().await {
                    tracing::error!(error = %ie, "config cache invalidate failed");
                }
            }
        }

        self.audit
            .log(
                operator,
                "publish",
                "config_version",
                Some(version.version),
                serde_json::json!({"version": version.version, "base": version.base_version}),
                Some(trace_id.to_string()),
            )
            .await;

        Ok(version)
    }

    pub async fn rollback_config(
        &self,
        target_version: u64,
        operator: Option<&str>,
        trace_id: &str,
    ) -> Result<ConfigVersionDto, ConrogateError> {
        // 1. 取目标快照
        let target_snapshot = self
            .config_repo
            .get_snapshot_by_version(target_version)
            .await?
            .ok_or_else(|| ConrogateError::NotFound(format!("version {}", target_version)))?;

        // 2. 回写业务表（gate 热加载直接读业务表，回滚依赖这一步生效）
        self.config_repo.apply_snapshot(&target_snapshot).await?;

        // 3. 写版本行
        let version = self.config_repo.rollback(target_version, operator).await?;

        // 写 Redis 配置缓存（失败不阻断回滚：使缓存失效并告警，
        // 数据面降级直连 DB 轮询）
        if let Some(ref cache) = self.config_cache {
            match self
                .config_repo
                .get_snapshot_by_version(version.version)
                .await?
            {
                Some(snapshot) => {
                    if let Err(e) = cache.put_snapshot(version.version, &snapshot).await {
                        tracing::error!(
                            version = version.version,
                            error = %e,
                            "rollback snapshot Redis cache write failed after retries, invalidating cache"
                        );
                        if let Err(ie) = cache.invalidate().await {
                            tracing::error!(error = %ie, "config cache invalidate failed");
                        }
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

        self.audit
            .log(
                operator,
                "rollback",
                "config_version",
                Some(version.version),
                serde_json::json!({"target": target_version, "new_version": version.version}),
                Some(trace_id.to_string()),
            )
            .await;

        Ok(version)
    }

    pub async fn list_config_versions(
        &self,
        page: u32,
        page_size: u32,
    ) -> Result<PaginatedResult<ConfigVersionDto>, ConrogateError> {
        self.config_repo.list_versions(page, page_size).await
    }

    pub async fn diff_config(&self, from: u64, to: u64) -> Result<ConfigDiff, ConrogateError> {
        self.config_repo.diff(from, to).await
    }

    // ── 指标查询 ──

    pub async fn query_metrics(
        &self,
        filter: MetricQuery,
    ) -> Result<Vec<MetricRow>, ConrogateError> {
        self.metric_repo.query(&filter).await
    }

    pub async fn overview_metrics(&self, range_min: u32) -> Result<OverviewMetric, ConrogateError> {
        self.metric_repo.overview(range_min).await
    }

    // ── Insights 聚合查询 ──

    pub async fn insights_qps(&self, range_min: u32) -> Result<serde_json::Value, ConrogateError> {
        let rows = self
            .metric_repo
            .query(&MetricQuery {
                range_min,
                route_id: None,
                gate_id: None,
            })
            .await?;
        let series: Vec<serde_json::Value> = rows
            .iter()
            .map(|r| {
                serde_json::json!({
                    "ts": r.ts, "qps": r.qps, "route_id": r.route_id
                })
            })
            .collect();
        Ok(serde_json::json!({ "series": series }))
    }

    pub async fn insights_latency(
        &self,
        range_min: u32,
    ) -> Result<serde_json::Value, ConrogateError> {
        let rows = self
            .metric_repo
            .query(&MetricQuery {
                range_min,
                route_id: None,
                gate_id: None,
            })
            .await?;
        let p50: f64 = if rows.is_empty() {
            0.0
        } else {
            rows.iter().map(|r| r.p50_ms as f64).sum::<f64>() / rows.len() as f64
        };
        let p90: f64 = if rows.is_empty() {
            0.0
        } else {
            rows.iter().map(|r| r.p90_ms as f64).sum::<f64>() / rows.len() as f64
        };
        let p99: f64 = if rows.is_empty() {
            0.0
        } else {
            rows.iter().map(|r| r.p99_ms as f64).sum::<f64>() / rows.len() as f64
        };
        let avg: f64 = if rows.is_empty() {
            0.0
        } else {
            rows.iter().map(|r| r.avg_latency_ms).sum::<f64>() / rows.len() as f64
        };
        Ok(serde_json::json!({ "avg_ms": avg, "p50_ms": p50, "p90_ms": p90, "p99_ms": p99 }))
    }

    pub async fn insights_status_codes(
        &self,
        range_min: u32,
    ) -> Result<serde_json::Value, ConrogateError> {
        let rows = self
            .metric_repo
            .query(&MetricQuery {
                range_min,
                route_id: None,
                gate_id: None,
            })
            .await?;
        let s2xx: u64 = rows.iter().map(|r| r.status_2xx).sum();
        let s3xx: u64 = rows.iter().map(|r| r.status_3xx).sum();
        let s4xx: u64 = rows.iter().map(|r| r.status_4xx).sum();
        let s5xx: u64 = rows.iter().map(|r| r.status_5xx).sum();
        Ok(serde_json::json!({ "2xx": s2xx, "3xx": s3xx, "4xx": s4xx, "5xx": s5xx }))
    }

    pub async fn insights_top_routes(
        &self,
        range_min: u32,
    ) -> Result<serde_json::Value, ConrogateError> {
        let rows = self
            .metric_repo
            .query(&MetricQuery {
                range_min,
                route_id: None,
                gate_id: None,
            })
            .await?;
        use std::collections::HashMap;
        let mut by_route: HashMap<u64, u64> = HashMap::new();
        for r in &rows {
            if let Some(rid) = r.route_id {
                *by_route.entry(rid).or_insert(0) += r.total_requests;
            }
        }
        let mut top: Vec<(u64, u64)> = by_route.into_iter().collect();
        top.sort_by_key(|(_, v)| std::cmp::Reverse(*v));
        top.truncate(10);
        let result: Vec<serde_json::Value> = top
            .iter()
            .map(|(rid, reqs)| {
                serde_json::json!({
                    "route_id": rid, "total_requests": reqs
                })
            })
            .collect();
        Ok(serde_json::json!({ "top_routes": result }))
    }

    // ── 事件查询 ──

    pub async fn query_events(
        &self,
        filter: EventQuery,
        page: u32,
        page_size: u32,
    ) -> Result<PaginatedResult<EventRow>, ConrogateError> {
        self.event_repo.query(&filter, page, page_size).await
    }

    // ── 审计查询 ──

    pub async fn query_audit_logs(
        &self,
        filter: AuditLogQuery,
        page: u32,
        page_size: u32,
    ) -> Result<PaginatedResult<AuditLogRow>, ConrogateError> {
        self.audit_repo.query(&filter, page, page_size).await
    }

    // ── 节点管理 ──

    pub async fn list_nodes(&self) -> Result<Vec<NodeApplicationRow>, ConrogateError> {
        self.node_app_repo.list_all().await
    }

    // ── 插件管理 ──

    pub async fn list_plugins(
        &self,
        status: Option<crate::contract::plugin::PluginStatus>,
    ) -> Result<Vec<InstalledPluginDto>, ConrogateError> {
        self.plugin_repo.list(status).await
    }

    /// 更新插件状态（Admin 专属操作）
    pub async fn update_plugin_status(
        &self,
        name: &str,
        status: crate::contract::plugin::PluginStatus,
        operator: Option<&str>,
        trace_id: &str,
    ) -> Result<(), ConrogateError> {
        self.plugin_repo.update_status(name, status).await?;
        self.audit
            .log(
                operator,
                "update_status",
                "plugin",
                None,
                serde_json::json!({"name": name, "status": format!("{:?}", status)}),
                Some(trace_id.to_string()),
            )
            .await;
        Ok(())
    }

    /// 卸载插件（Admin 专属操作）
    pub async fn delete_plugin(
        &self,
        name: &str,
        operator: Option<&str>,
        trace_id: &str,
    ) -> Result<(), ConrogateError> {
        self.plugin_repo.soft_delete(name).await?;
        self.audit
            .log(
                operator,
                "delete",
                "plugin",
                None,
                serde_json::json!({"name": name}),
                Some(trace_id.to_string()),
            )
            .await;
        Ok(())
    }

    // ── 全局 IP 黑名单 ──

    pub async fn list_ip_blacklist(
        &self,
        filter: IpBlacklistQuery,
        page: u32,
        page_size: u32,
    ) -> Result<PaginatedResult<IpBlacklistDto>, ConrogateError> {
        self.ip_blacklist_repo
            .list_paginated(&filter, page, page_size)
            .await
    }

    /// 拉黑 IP/CIDR（合法网段校验 + 幂等 upsert）
    pub async fn create_ip_blacklist(
        &self,
        dto: CreateIpBlacklistDto,
        operator: Option<&str>,
        trace_id: &str,
    ) -> Result<IpBlacklistDto, ConrogateError> {
        let ip_or_cidr = dto.ip_or_cidr.trim().to_string();
        if crate::security::blacklist::parse_ip_or_cidr(&ip_or_cidr).is_none() {
            return Err(ConrogateError::BadRequest(format!(
                "非法 IP 或 CIDR: {ip_or_cidr}"
            )));
        }
        if dto.expires_in_seconds.is_some_and(|s| s == 0) {
            return Err(ConrogateError::BadRequest(
                "expires_in_seconds 必须大于 0".into(),
            ));
        }
        let entry = self
            .ip_blacklist_repo
            .upsert(
                &CreateIpBlacklistDto {
                    ip_or_cidr,
                    reason: dto
                        .reason
                        .map(|r| r.trim().to_string())
                        .filter(|r| !r.is_empty()),
                    expires_in_seconds: dto.expires_in_seconds,
                },
                operator,
            )
            .await?;
        self.audit
            .log(
                operator,
                "create",
                "ip_blacklist",
                Some(entry.id),
                serde_json::to_value(&entry).unwrap_or_default(),
                Some(trace_id.to_string()),
            )
            .await;
        Ok(entry)
    }

    /// 解除拉黑
    pub async fn delete_ip_blacklist(
        &self,
        id: u64,
        operator: Option<&str>,
        trace_id: &str,
    ) -> Result<(), ConrogateError> {
        self.ip_blacklist_repo.delete(id).await?;
        self.audit
            .log(
                operator,
                "delete",
                "ip_blacklist",
                Some(id),
                serde_json::json!({"id": id}),
                Some(trace_id.to_string()),
            )
            .await;
        Ok(())
    }

    pub async fn receive_heartbeat(&self, heartbeat: Heartbeat) -> Result<(), ConrogateError> {
        self.node_app_repo
            .upsert(&heartbeat.gate_id, heartbeat.version, heartbeat.timestamp)
            .await
    }

    pub async fn receive_metrics(&self, batch: MetricsBatch) -> Result<(), ConrogateError> {
        if batch.metrics.len() > REPORT_BATCH_LIMIT {
            return Err(ConrogateError::BadRequest(format!(
                "批量上报超过上限 {} 条，实际 {} 条",
                REPORT_BATCH_LIMIT,
                batch.metrics.len()
            )));
        }
        if batch.bucket_sec != 10 && batch.bucket_sec != 60 {
            return Err(ConrogateError::BadRequest(format!(
                "bucket_sec 仅支持 10/60，实际 {}",
                batch.bucket_sec
            )));
        }
        self.metric_repo.upsert_batch(&batch.metrics).await
    }

    pub async fn receive_events(&self, batch: EventsBatch) -> Result<(), ConrogateError> {
        if batch.events.len() > REPORT_BATCH_LIMIT {
            return Err(ConrogateError::BadRequest(format!(
                "批量上报超过上限 {} 条，实际 {} 条",
                REPORT_BATCH_LIMIT,
                batch.events.len()
            )));
        }
        // 幂等去重：trace_id + ts + event_type（无 trace_id 的事件不参与去重）
        let mut seen = std::collections::HashSet::new();
        let deduped: Vec<EventRow> = batch
            .events
            .into_iter()
            .filter(|e| match &e.trace_id {
                Some(tid) => seen.insert((tid.clone(), e.ts, e.event_type.clone())),
                None => true,
            })
            .collect();
        self.event_repo.insert_batch(&deduped).await
    }
}
