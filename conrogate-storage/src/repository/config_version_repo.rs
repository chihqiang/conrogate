//! 配置版本仓储实现。

use crate::convert;
use crate::entity::{
    config_versions::{self, Entity as ConfigVersionEntity},
    route_plugin_bindings, routes, upstream_nodes, upstreams,
};
use conrogate_contract::dto::*;
use conrogate_contract::storage::ConfigVersionRepo;
use conrogate_contract::ConrogateError;
use sea_orm::{
    sea_query::Expr, ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
};
use std::collections::{HashMap, HashSet};

pub struct ConfigVersionRepoImpl {
    db: DatabaseConnection,
}

impl ConfigVersionRepoImpl {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl ConfigVersionRepo for ConfigVersionRepoImpl {
    async fn publish(
        &self,
        base_version: u64,
        snapshot: &ConfigSnapshot,
        created_by: Option<&str>,
        remark: Option<&str>,
    ) -> Result<ConfigVersionDto, ConrogateError> {
        let latest = ConfigVersionEntity::find()
            .order_by_desc(config_versions::Column::Version)
            .one(&self.db)
            .await
            .map_err(|_| ConrogateError::DatabaseInternal)?;

        let current_latest = latest.map(|m| m.version).unwrap_or(0);
        if base_version != current_latest as u64 {
            return Err(ConrogateError::ConfigConcurrencyConflict);
        }

        let new_version = current_latest + 1;
        let snapshot_json = serde_json::to_value(snapshot)
            .map_err(|e| ConrogateError::DataMapping(e.to_string()))?;

        let content_hash = format!("sha256:{}", sha256_hash(&snapshot_json.to_string()));
        let now = chrono::Utc::now();

        let active = config_versions::ActiveModel {
            version: Set(new_version),
            base_version: Set(base_version as i64),
            publish_type: Set(0),
            content_hash: Set(content_hash),
            snapshot_content: Set(snapshot_json),
            created_by: Set(created_by.map(|s| s.to_string())),
            remark: Set(remark.map(|s| s.to_string())),
            created_at: Set(now),
            ..Default::default()
        };

        let model = active
            .insert(&self.db)
            .await
            .map_err(|e| ConrogateError::DataMapping(e.to_string()))?;

        convert::config_version_model_to_dto(model).ok_or(ConrogateError::DataMapping(
            "insert returned no model".into(),
        ))
    }

    async fn list_versions(
        &self,
        page: u32,
        page_size: u32,
    ) -> Result<PaginatedResult<ConfigVersionDto>, ConrogateError> {
        let page_size = page_size.clamp(1, 200);
        let query = ConfigVersionEntity::find().order_by_desc(config_versions::Column::Version);

        let total = query
            .clone()
            .count(&self.db)
            .await
            .map_err(|_| ConrogateError::DatabaseInternal)?;

        let models = query
            .offset(((page - 1) * page_size) as u64)
            .limit(page_size as u64)
            .all(&self.db)
            .await
            .map_err(|_| ConrogateError::DatabaseInternal)?;

        let list: Vec<ConfigVersionDto> = models
            .into_iter()
            .filter_map(convert::config_version_model_to_dto)
            .collect();
        Ok(PaginatedResult {
            list,
            total,
            page,
            page_size,
        })
    }

    async fn find_by_version(
        &self,
        version: u64,
    ) -> Result<Option<ConfigVersionDto>, ConrogateError> {
        let model = ConfigVersionEntity::find()
            .filter(config_versions::Column::Version.eq(version as i64))
            .one(&self.db)
            .await
            .map_err(|_| ConrogateError::DatabaseInternal)?;

        Ok(model.and_then(convert::config_version_model_to_dto))
    }

    async fn latest_version(&self) -> Result<Option<ConfigVersionDto>, ConrogateError> {
        let model = ConfigVersionEntity::find()
            .order_by_desc(config_versions::Column::Version)
            .one(&self.db)
            .await
            .map_err(|_| ConrogateError::DatabaseInternal)?;

        Ok(model.and_then(convert::config_version_model_to_dto))
    }

    async fn rollback(
        &self,
        target_version: u64,
        created_by: Option<&str>,
    ) -> Result<ConfigVersionDto, ConrogateError> {
        let target = ConfigVersionEntity::find()
            .filter(config_versions::Column::Version.eq(target_version as i64))
            .one(&self.db)
            .await
            .map_err(|_| ConrogateError::DatabaseInternal)?
            .ok_or_else(|| ConrogateError::NotFound(format!("version {}", target_version)))?;

        let latest = self.latest_version().await?;
        let current_latest = latest.map(|d| d.version).unwrap_or(0);
        let new_version = current_latest + 1;
        let now = chrono::Utc::now();

        let active = config_versions::ActiveModel {
            version: Set(new_version as i64),
            base_version: Set(current_latest as i64),
            publish_type: Set(1),
            content_hash: Set(target.content_hash.clone()),
            snapshot_content: Set(target.snapshot_content.clone()),
            created_by: Set(created_by.map(|s| s.to_string())),
            remark: Set(Some(format!("rollback to v{}", target_version))),
            created_at: Set(now),
            ..Default::default()
        };

        let model = active
            .insert(&self.db)
            .await
            .map_err(|e| ConrogateError::DataMapping(e.to_string()))?;

        convert::config_version_model_to_dto(model).ok_or(ConrogateError::DataMapping(
            "insert returned no model".into(),
        ))
    }

    async fn get_snapshot_by_version(
        &self,
        version: u64,
    ) -> Result<Option<ConfigSnapshot>, ConrogateError> {
        let model = ConfigVersionEntity::find()
            .filter(config_versions::Column::Version.eq(version as i64))
            .one(&self.db)
            .await
            .map_err(|_| ConrogateError::DatabaseInternal)?;

        match model {
            Some(m) => {
                let snap: ConfigSnapshot = serde_json::from_value(m.snapshot_content)
                    .map_err(|e| ConrogateError::DataMapping(e.to_string()))?;
                Ok(Some(snap))
            }
            None => Ok(None),
        }
    }

    async fn apply_snapshot(&self, snapshot: &ConfigSnapshot) -> Result<(), ConrogateError> {
        let tx = self
            .db
            .begin()
            .await
            .map_err(|_| ConrogateError::DatabaseInternal)?;

        // ── 1. 上游 upsert（按 name 匹配），建立 快照 id → 实际 id 映射 ──
        let mut upstream_id_map: HashMap<u64, i64> = HashMap::new();
        for up in &snapshot.upstreams {
            let existing = upstreams::Entity::find()
                .filter(upstreams::Column::Name.eq(&up.name))
                .filter(upstreams::Column::DeletedAt.is_null())
                .one(&tx)
                .await
                .map_err(|_| ConrogateError::DatabaseInternal)?;

            let actual_id = match existing {
                Some(m) => {
                    let mut active: upstreams::ActiveModel = m.clone().into();
                    active.algorithm = Set(convert::algorithm_to_i16(up.algorithm));
                    active.retry_enabled = Set(up.retry_enabled);
                    active.updated_at = Set(chrono::Utc::now());
                    active
                        .update(&tx)
                        .await
                        .map_err(|_| ConrogateError::DatabaseInternal)?;

                    // 替换节点：软删旧节点 + 插入快照节点
                    upstream_nodes::Entity::update_many()
                        .col_expr(
                            upstream_nodes::Column::DeletedAt,
                            Expr::value(Some(chrono::Utc::now())),
                        )
                        .filter(upstream_nodes::Column::UpstreamId.eq(m.id))
                        .filter(upstream_nodes::Column::DeletedAt.is_null())
                        .exec(&tx)
                        .await
                        .map_err(|_| ConrogateError::DatabaseInternal)?;
                    for node in &up.nodes {
                        let node_active = convert::node_create_to_active_model(
                            m.id,
                            CreateUpstreamNodeDto {
                                address: node.address.clone(),
                                weight: Some(node.weight),
                                enabled: Some(node.enabled),
                            },
                        );
                        node_active
                            .insert(&tx)
                            .await
                            .map_err(|_| ConrogateError::DatabaseInternal)?;
                    }
                    m.id
                }
                None => {
                    let active = convert::upstream_create_to_active_model(CreateUpstreamDto {
                        name: up.name.clone(),
                        algorithm: up.algorithm,
                        retry_enabled: Some(up.retry_enabled),
                        nodes: up
                            .nodes
                            .iter()
                            .map(|n| CreateUpstreamNodeDto {
                                address: n.address.clone(),
                                weight: Some(n.weight),
                                enabled: Some(n.enabled),
                            })
                            .collect(),
                    });
                    let model = active
                        .insert(&tx)
                        .await
                        .map_err(|_| ConrogateError::DatabaseInternal)?;
                    model.id
                }
            };
            upstream_id_map.insert(up.id, actual_id);
        }

        // ── 2. 软删不在快照中的上游（含节点）──
        let snapshot_upstream_names: HashSet<String> =
            snapshot.upstreams.iter().map(|u| u.name.clone()).collect();
        let current_upstreams = upstreams::Entity::find()
            .filter(upstreams::Column::DeletedAt.is_null())
            .all(&tx)
            .await
            .map_err(|_| ConrogateError::DatabaseInternal)?;
        for m in &current_upstreams {
            if !snapshot_upstream_names.contains(&m.name) {
                upstreams::Entity::update_many()
                    .col_expr(
                        upstreams::Column::DeletedAt,
                        Expr::value(Some(chrono::Utc::now())),
                    )
                    .filter(upstreams::Column::Id.eq(m.id))
                    .filter(upstreams::Column::DeletedAt.is_null())
                    .exec(&tx)
                    .await
                    .map_err(|_| ConrogateError::DatabaseInternal)?;
                upstream_nodes::Entity::update_many()
                    .col_expr(
                        upstream_nodes::Column::DeletedAt,
                        Expr::value(Some(chrono::Utc::now())),
                    )
                    .filter(upstream_nodes::Column::UpstreamId.eq(m.id))
                    .filter(upstream_nodes::Column::DeletedAt.is_null())
                    .exec(&tx)
                    .await
                    .map_err(|_| ConrogateError::DatabaseInternal)?;
            }
        }

        // ── 3. 路由 upsert（按 name 匹配），upstream_id 经快照 id → 实际 id 重映射 ──
        let mut route_id_map: HashMap<u64, i64> = HashMap::new();
        for r in &snapshot.routes {
            let upstream_id: Option<i64> = match r.upstream_id {
                Some(sid) => snapshot
                    .upstreams
                    .iter()
                    .find(|u| u.id == sid)
                    .and_then(|u| upstream_id_map.get(&u.id))
                    .copied(),
                None => None,
            };

            let existing = routes::Entity::find()
                .filter(routes::Column::Name.eq(&r.name))
                .filter(routes::Column::DeletedAt.is_null())
                .one(&tx)
                .await
                .map_err(|_| ConrogateError::DatabaseInternal)?;

            let actual_id = match existing {
                Some(m) => {
                    let mut active: routes::ActiveModel = m.clone().into();
                    active.protocol = Set(convert::protocol_to_i16(r.protocol));
                    active.match_conditions = Set(serde_json::to_value(&r.match_conditions)
                        .map_err(|e| ConrogateError::DataMapping(e.to_string()))?);
                    active.priority = Set(r.priority);
                    active.upstream_id = Set(upstream_id);
                    active.host_header = Set(r.host_header.clone());
                    active.allow_retry_non_idempotent = Set(r.allow_retry_non_idempotent);
                    active.ws_strip_sensitive_headers = Set(r.ws_strip_sensitive_headers);
                    active.enabled = Set(r.enabled);
                    active.updated_at = Set(chrono::Utc::now());
                    active
                        .update(&tx)
                        .await
                        .map_err(|_| ConrogateError::DatabaseInternal)?;
                    m.id
                }
                None => {
                    let active = convert::route_create_to_active_model(CreateRouteDto {
                        name: r.name.clone(),
                        protocol: r.protocol,
                        match_conditions: r.match_conditions.clone(),
                        priority: Some(r.priority),
                        upstream_id: upstream_id.map(|v| v as u64),
                        host_header: r.host_header.clone(),
                        allow_retry_non_idempotent: Some(r.allow_retry_non_idempotent),
                        ws_strip_sensitive_headers: Some(r.ws_strip_sensitive_headers),
                        enabled: Some(r.enabled),
                    });
                    let model = active
                        .insert(&tx)
                        .await
                        .map_err(|_| ConrogateError::DatabaseInternal)?;
                    model.id
                }
            };
            route_id_map.insert(r.id, actual_id);
        }

        // ── 4. 软删不在快照中的活跃路由（禁用路由保留，避免误删）──
        let snapshot_route_names: HashSet<String> =
            snapshot.routes.iter().map(|r| r.name.clone()).collect();
        let current_routes = routes::Entity::find()
            .filter(routes::Column::DeletedAt.is_null())
            .filter(routes::Column::Enabled.eq(true))
            .all(&tx)
            .await
            .map_err(|_| ConrogateError::DatabaseInternal)?;
        let mut deleted_route_ids: Vec<i64> = Vec::new();
        for m in &current_routes {
            if !snapshot_route_names.contains(&m.name) {
                routes::Entity::update_many()
                    .col_expr(
                        routes::Column::DeletedAt,
                        Expr::value(Some(chrono::Utc::now())),
                    )
                    .filter(routes::Column::Id.eq(m.id))
                    .filter(routes::Column::DeletedAt.is_null())
                    .exec(&tx)
                    .await
                    .map_err(|_| ConrogateError::DatabaseInternal)?;
                deleted_route_ids.push(m.id);
            }
        }

        // ── 5. 插件绑定对齐 ──
        // 5a. 清理被软删路由的绑定
        for rid in &deleted_route_ids {
            route_plugin_bindings::Entity::update_many()
                .col_expr(
                    route_plugin_bindings::Column::DeletedAt,
                    Expr::value(Some(chrono::Utc::now())),
                )
                .filter(route_plugin_bindings::Column::RouteId.eq(*rid))
                .filter(route_plugin_bindings::Column::DeletedAt.is_null())
                .exec(&tx)
                .await
                .map_err(|_| ConrogateError::DatabaseInternal)?;
        }

        // 5b. 快照路由绑定 upsert（route_id 经重映射）
        let mut desired_bindings: HashSet<(i64, String)> = HashSet::new();
        for b in &snapshot.plugin_bindings {
            let actual_route_id = route_id_map.get(&b.route_id).ok_or_else(|| {
                ConrogateError::DataMapping(format!(
                    "binding route {} not found in snapshot routes",
                    b.route_id
                ))
            })?;
            let existing = route_plugin_bindings::Entity::find()
                .filter(route_plugin_bindings::Column::RouteId.eq(*actual_route_id))
                .filter(route_plugin_bindings::Column::PluginName.eq(&b.plugin_name))
                .filter(route_plugin_bindings::Column::DeletedAt.is_null())
                .one(&tx)
                .await
                .map_err(|_| ConrogateError::DatabaseInternal)?;
            match existing {
                Some(m) => {
                    let mut active: route_plugin_bindings::ActiveModel = m.clone().into();
                    active.config = Set(b.config.clone());
                    active.order = Set(b.order);
                    active.blocking = Set(b.blocking);
                    active.enabled = Set(b.enabled);
                    active.updated_at = Set(chrono::Utc::now());
                    active
                        .update(&tx)
                        .await
                        .map_err(|_| ConrogateError::DatabaseInternal)?;
                }
                None => {
                    let active = convert::binding_create_to_active_model(
                        *actual_route_id,
                        BindPluginDto {
                            plugin_name: b.plugin_name.clone(),
                            config: b.config.clone(),
                            order: Some(b.order),
                            blocking: Some(b.blocking),
                            enabled: Some(b.enabled),
                        },
                    );
                    active
                        .insert(&tx)
                        .await
                        .map_err(|_| ConrogateError::DatabaseInternal)?;
                }
            }
            desired_bindings.insert((*actual_route_id, b.plugin_name.clone()));
        }

        // 5c. 清理快照路由上已不在快照中的绑定
        for actual_route_id in route_id_map.values() {
            let current_bindings = route_plugin_bindings::Entity::find()
                .filter(route_plugin_bindings::Column::RouteId.eq(*actual_route_id))
                .filter(route_plugin_bindings::Column::DeletedAt.is_null())
                .all(&tx)
                .await
                .map_err(|_| ConrogateError::DatabaseInternal)?;
            for m in &current_bindings {
                if !desired_bindings.contains(&(*actual_route_id, m.plugin_name.clone())) {
                    route_plugin_bindings::Entity::update_many()
                        .col_expr(
                            route_plugin_bindings::Column::DeletedAt,
                            Expr::value(Some(chrono::Utc::now())),
                        )
                        .filter(route_plugin_bindings::Column::Id.eq(m.id))
                        .filter(route_plugin_bindings::Column::DeletedAt.is_null())
                        .exec(&tx)
                        .await
                        .map_err(|_| ConrogateError::DatabaseInternal)?;
                }
            }
        }

        tx.commit()
            .await
            .map_err(|_| ConrogateError::DatabaseInternal)?;
        Ok(())
    }

    async fn diff(&self, from: u64, to: u64) -> Result<ConfigDiff, ConrogateError> {
        let from_model = ConfigVersionEntity::find()
            .filter(config_versions::Column::Version.eq(from as i64))
            .one(&self.db)
            .await
            .map_err(|_| ConrogateError::DatabaseInternal)?;

        let to_model = ConfigVersionEntity::find()
            .filter(config_versions::Column::Version.eq(to as i64))
            .one(&self.db)
            .await
            .map_err(|_| ConrogateError::DatabaseInternal)?;

        match (from_model, to_model) {
            (Some(from_m), Some(to_m)) => {
                let from_snap: ConfigSnapshot = serde_json::from_value(from_m.snapshot_content)
                    .map_err(|e| ConrogateError::DataMapping(e.to_string()))?;
                let to_snap: ConfigSnapshot = serde_json::from_value(to_m.snapshot_content)
                    .map_err(|e| ConrogateError::DataMapping(e.to_string()))?;

                let from_names: std::collections::HashSet<String> =
                    from_snap.routes.iter().map(|r| r.name.clone()).collect();
                let to_names: std::collections::HashSet<String> =
                    to_snap.routes.iter().map(|r| r.name.clone()).collect();

                let added: Vec<String> = to_names.difference(&from_names).cloned().collect();
                let removed: Vec<String> = from_names.difference(&to_names).cloned().collect();
                let modified: Vec<String> = to_snap
                    .routes
                    .iter()
                    .filter(|r| from_names.contains(&r.name))
                    .map(|r| r.name.clone())
                    .collect();

                Ok(ConfigDiff {
                    added,
                    modified,
                    removed,
                })
            }
            _ => Err(ConrogateError::NotFound("version not found".into())),
        }
    }
}

/// 计算字符串的 SHA-256 哈希（返回 64 位十六进制字符串）
fn sha256_hash(s: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    let result = hasher.finalize();
    // 转为十六进制字符串
    hex::encode(result)
}
