//! config_versions 表实体。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "config_versions")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub version: i64,
    #[sea_orm(column_name = "base_version")]
    pub base_version: i64,
    #[sea_orm(column_name = "publish_type")]
    pub publish_type: i16,
    #[sea_orm(column_name = "content_hash")]
    pub content_hash: String,
    #[sea_orm(column_type = "Json")]
    pub snapshot_content: Json,
    #[sea_orm(column_name = "created_by", nullable)]
    pub created_by: Option<String>,
    #[sea_orm(nullable)]
    pub remark: Option<String>,
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
