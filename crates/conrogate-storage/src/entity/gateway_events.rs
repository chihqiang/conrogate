//! gateway_events 表实体。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "gateway_events")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub ts: DateTimeUtc,
    #[sea_orm(column_name = "event_type")]
    pub event_type: String,
    #[sea_orm(column_name = "route_id", nullable)]
    pub route_id: Option<i64>,
    #[sea_orm(column_name = "upstream_id", nullable)]
    pub upstream_id: Option<i64>,
    #[sea_orm(column_name = "trace_id", nullable)]
    pub trace_id: Option<String>,
    #[sea_orm(column_type = "Json", nullable)]
    pub detail: Option<Json>,
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
