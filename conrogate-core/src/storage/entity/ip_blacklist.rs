//! ip_blacklist 表实体。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "ip_blacklist")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(column_name = "ip_or_cidr")]
    pub ip_or_cidr: String,
    #[sea_orm(nullable)]
    pub reason: Option<String>,
    #[sea_orm(column_name = "expires_at", nullable)]
    pub expires_at: Option<DateTimeUtc>,
    #[sea_orm(column_name = "created_by", nullable)]
    pub created_by: Option<String>,
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
