//! 审计日志仓储实现。

use crate::convert;
use crate::entity::audit_logs::{self, Entity as AuditEntity};
use conrogate_contract::dto::{AuditLogQuery, AuditLogRow, PaginatedResult};
use conrogate_contract::storage::AuditLogRepo;
use conrogate_contract::ConrogateError;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder, QuerySelect,
};

pub struct AuditLogRepoImpl {
    db: DatabaseConnection,
}

impl AuditLogRepoImpl {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl AuditLogRepo for AuditLogRepoImpl {
    async fn insert(&self, row: &AuditLogRow) -> Result<(), ConrogateError> {
        let active = convert::audit_row_to_active_model(row);
        active
            .insert(&self.db)
            .await
            .map_err(|e| ConrogateError::DataMapping(e.to_string()))?;
        Ok(())
    }

    async fn query(
        &self,
        filter: &AuditLogQuery,
        page: u32,
        page_size: u32,
    ) -> Result<PaginatedResult<AuditLogRow>, ConrogateError> {
        let page_size = page_size.clamp(1, 200);
        let mut query = AuditEntity::find().order_by_desc(audit_logs::Column::Ts);

        if let Some(ref operator) = filter.operator {
            query = query.filter(audit_logs::Column::Operator.eq(operator));
        }
        if let Some(ref action) = filter.action {
            query = query.filter(audit_logs::Column::Action.eq(action));
        }
        if let Some(ref resource) = filter.resource {
            query = query.filter(audit_logs::Column::Resource.eq(resource));
        }
        if let Some(ts_from) = filter.ts_from {
            query = query.filter(audit_logs::Column::Ts.gte(ts_from));
        }
        if let Some(ts_to) = filter.ts_to {
            query = query.filter(audit_logs::Column::Ts.lte(ts_to));
        }

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

        let list: Vec<AuditLogRow> = models
            .into_iter()
            .filter_map(convert::audit_model_to_row)
            .collect();
        Ok(PaginatedResult {
            list,
            total,
            page,
            page_size,
        })
    }
}
