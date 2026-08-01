//! route_plugin_bindings 表实体。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "route_plugin_bindings")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(column_name = "route_id")]
    pub route_id: i64,
    #[sea_orm(column_name = "plugin_name")]
    pub plugin_name: String,
    #[sea_orm(column_type = "Json")]
    pub config: Json,
    #[sea_orm(column_name = "order")]
    pub order: i32,
    pub blocking: bool,
    pub enabled: bool,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
    #[sea_orm(nullable)]
    pub deleted_at: Option<DateTimeUtc>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::routes::Entity",
        from = "Column::RouteId",
        to = "super::routes::Column::Id"
    )]
    Route,
}

impl Related<super::routes::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Route.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
