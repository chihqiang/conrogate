//! installed_plugins 表实体。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "installed_plugins")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub name: String,
    pub version: String,
    #[sea_orm(column_name = "api_version")]
    pub api_version: i32,
    #[sea_orm(column_name = "kind")]
    pub kind: i16,
    #[sea_orm(column_name = "status")]
    pub status: i16,
    #[sea_orm(column_name = "package_hash", nullable)]
    pub package_hash: Option<String>,
    #[sea_orm(column_type = "Json")]
    pub manifest: Json,
    #[sea_orm(column_name = "installed_at")]
    pub installed_at: DateTimeUtc,
    #[sea_orm(column_name = "activated_at", nullable)]
    pub activated_at: Option<DateTimeUtc>,
    #[sea_orm(nullable)]
    pub deleted_at: Option<DateTimeUtc>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
