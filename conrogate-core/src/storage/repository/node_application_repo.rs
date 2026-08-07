//! 节点应用记录仓储实现。

use crate::contract::dto::NodeApplicationRow;
use crate::contract::storage::NodeApplicationRepo;
use crate::contract::ConrogateError;
use crate::storage::convert;
use crate::storage::entity::node_applications::{self, Entity as NodeAppEntity};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder, Set,
};

pub struct NodeApplicationRepoImpl {
    db: DatabaseConnection,
}

impl NodeApplicationRepoImpl {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl NodeApplicationRepo for NodeApplicationRepoImpl {
    async fn upsert(
        &self,
        gate_id: &str,
        version: u64,
        last_seen: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), ConrogateError> {
        let existing = NodeAppEntity::find()
            .filter(node_applications::Column::GateId.eq(gate_id))
            .one(&self.db)
            .await
            .map_err(|_| ConrogateError::DatabaseInternal)?;

        let now = chrono::Utc::now();
        match existing {
            Some(model) => {
                let mut active: node_applications::ActiveModel = model.into();
                active.version = Set(version as i64);
                active.last_seen = Set(last_seen);
                active.updated_at = Set(now);
                active
                    .update(&self.db)
                    .await
                    .map_err(|e| ConrogateError::DataMapping(e.to_string()))?;
            }
            None => {
                let active = node_applications::ActiveModel {
                    gate_id: Set(gate_id.to_string()),
                    version: Set(version as i64),
                    applied_at: Set(now),
                    last_seen: Set(last_seen),
                    updated_at: Set(now),
                    ..Default::default()
                };
                active
                    .insert(&self.db)
                    .await
                    .map_err(|e| ConrogateError::DataMapping(e.to_string()))?;
            }
        }
        Ok(())
    }

    async fn count_by_version(&self, version: u64) -> Result<u32, ConrogateError> {
        let count = NodeAppEntity::find()
            .filter(node_applications::Column::Version.eq(version as i64))
            .count(&self.db)
            .await
            .map_err(|_| ConrogateError::DatabaseInternal)?;
        Ok(count as u32)
    }

    async fn list_all(&self) -> Result<Vec<NodeApplicationRow>, ConrogateError> {
        let models = NodeAppEntity::find()
            .order_by_desc(node_applications::Column::UpdatedAt)
            .all(&self.db)
            .await
            .map_err(|_| ConrogateError::DatabaseInternal)?;
        Ok(models
            .into_iter()
            .filter_map(convert::node_app_model_to_row)
            .collect())
    }

    async fn list_stale(
        &self,
        before: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<NodeApplicationRow>, ConrogateError> {
        let models = NodeAppEntity::find()
            .filter(node_applications::Column::LastSeen.lt(before))
            .all(&self.db)
            .await
            .map_err(|_| ConrogateError::DatabaseInternal)?;
        Ok(models
            .into_iter()
            .filter_map(convert::node_app_model_to_row)
            .collect())
    }
}
