//! upstreams 表实体。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "upstreams")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub name: String,
    #[sea_orm(column_name = "algorithm")]
    pub algorithm: i16,
    pub retry_enabled: bool,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
    #[sea_orm(nullable)]
    pub deleted_at: Option<DateTimeUtc>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::upstream_nodes::Entity")]
    UpstreamNodes,
    #[sea_orm(has_many = "super::routes::Entity")]
    Routes,
}

impl Related<super::upstream_nodes::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::UpstreamNodes.def()
    }
}

impl Related<super::routes::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Routes.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
