//! 配置版本仓储实现。

use crate::convert;
use crate::entity::config_versions::{self, Entity as ConfigVersionEntity};
use conrogate_contract::dto::*;
use conrogate_contract::storage::ConfigVersionRepo;
use conrogate_contract::ConrogateError;
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set, RelationTrait};
use sea_orm::sea_query::Expr;

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

        let content_hash = format!("sha256:{:x}", simple_hash(&snapshot_json.to_string()));
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

        convert::config_version_model_to_dto(model)
            .ok_or(ConrogateError::DataMapping("insert returned no model".into()))
    }

    async fn list_versions(
        &self,
        page: u32,
        page_size: u32,
    ) -> Result<PaginatedResult<ConfigVersionDto>, ConrogateError> {
        let page_size = page_size.clamp(1, 200);
        let query = ConfigVersionEntity::find()
            .order_by_desc(config_versions::Column::Version);

        let total = query.clone().count(&self.db).await.map_err(|_| ConrogateError::DatabaseInternal)?;

        let models = query
            .offset(((page - 1) * page_size) as u64)
            .limit(page_size as u64)
            .all(&self.db)
            .await
            .map_err(|_| ConrogateError::DatabaseInternal)?;

        let list: Vec<ConfigVersionDto> = models.into_iter().filter_map(convert::config_version_model_to_dto).collect();
        Ok(PaginatedResult { list, total, page, page_size })
    }

    async fn find_by_version(&self, version: u64) -> Result<Option<ConfigVersionDto>, ConrogateError> {
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

        convert::config_version_model_to_dto(model)
            .ok_or(ConrogateError::DataMapping("insert returned no model".into()))
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

                let from_names: std::collections::HashSet<String> = from_snap.routes.iter().map(|r| r.name.clone()).collect();
                let to_names: std::collections::HashSet<String> = to_snap.routes.iter().map(|r| r.name.clone()).collect();

                let added: Vec<String> = to_names.difference(&from_names).cloned().collect();
                let removed: Vec<String> = from_names.difference(&to_names).cloned().collect();
                let modified: Vec<String> = to_snap.routes.iter()
                    .filter(|r| from_names.contains(&r.name))
                    .map(|r| r.name.clone())
                    .collect();

                Ok(ConfigDiff { added, modified, removed })
            }
            _ => Err(ConrogateError::NotFound("version not found".into())),
        }
    }
}

fn simple_hash(s: &str) -> u128 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish() as u128
}
