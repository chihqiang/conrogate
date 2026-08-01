//! routes 表实体。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "routes")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub name: String,
    #[sea_orm(column_name = "protocol")]
    pub protocol: i16,
    #[sea_orm(column_type = "Json")]
    pub match_conditions: Json,
    pub priority: i32,
    #[sea_orm(column_name = "upstream_id", nullable)]
    pub upstream_id: Option<i64>,
    #[sea_orm(column_name = "host_header", nullable)]
    pub host_header: Option<String>,
    #[sea_orm(column_name = "allow_retry_non_idempotent")]
    pub allow_retry_non_idempotent: bool,
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
    #[sea_orm(has_many = "super::route_plugin_bindings::Entity")]
    PluginBindings,
}

impl Related<super::upstreams::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Upstream.def()
    }
}

impl Related<super::route_plugin_bindings::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::PluginBindings.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
