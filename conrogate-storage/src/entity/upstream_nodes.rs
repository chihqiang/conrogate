//! upstream_nodes 表实体。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "upstream_nodes")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(column_name = "upstream_id")]
    pub upstream_id: i64,
    pub address: String,
    pub weight: i32,
    pub enabled: bool,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
    #[sea_orm(nullable)]
    pub deleted_at: Option<DateTimeUtc>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::upstreams::Entity",
        from = "Column::UpstreamId",
        to = "super::upstreams::Column::Id"
    )]
    Upstream,
}

impl Related<super::upstreams::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Upstream.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
