//! audit_logs 表实体。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "audit_logs")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub ts: DateTimeUtc,
    #[sea_orm(nullable)]
    pub operator: Option<String>,
    pub action: String,
    pub resource: String,
    #[sea_orm(column_name = "resource_id", nullable)]
    pub resource_id: Option<i64>,
    #[sea_orm(column_type = "Json", nullable)]
    pub detail: Option<Json>,
    #[sea_orm(column_name = "trace_id", nullable)]
    pub trace_id: Option<String>,
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
