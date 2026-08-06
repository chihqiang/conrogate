//! node_applications 表实体。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "node_applications")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(column_name = "gate_id", unique)]
    pub gate_id: String,
    pub version: i64,
    #[sea_orm(column_name = "applied_at")]
    pub applied_at: DateTimeUtc,
    #[sea_orm(column_name = "last_seen")]
    pub last_seen: DateTimeUtc,
    #[sea_orm(column_name = "updated_at")]
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
