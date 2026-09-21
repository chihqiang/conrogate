//! 迁移：路由插件绑定表。

use sea_orm::DatabaseBackend;
use sea_orm_migration::prelude::*;
use sea_orm_migration::MigrationName;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260101_000004_route_plugin_bindings"
    }
}

#[derive(DeriveIden)]
enum RoutePluginBindings {
    Table,
    Id,
    RouteId,
    PluginName,
    Config,
    Order,
    Blocking,
    Enabled,
    CreatedAt,
    UpdatedAt,
    DeletedAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(RoutePluginBindings::Table)
                    .col(
                        ColumnDef::new(RoutePluginBindings::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key()
                            .comment("绑定主键"),
                    )
                    .col(
                        ColumnDef::new(RoutePluginBindings::RouteId)
                            .big_integer()
                            .not_null()
                            .comment("路由 ID"),
                    )
                    .col(
                        ColumnDef::new(RoutePluginBindings::PluginName)
                            .string()
                            .not_null()
                            .comment("插件名"),
                    )
                    .col(
                        ColumnDef::new(RoutePluginBindings::Config)
                            .json()
                            .not_null()
                            .comment("插件配置 JSON（无 DB 默认值：MySQL JSON 列不支持默认值，由应用层写入）"),
                    )
                    .col(
                        ColumnDef::new(RoutePluginBindings::Order)
                            .integer()
                            .not_null()
                            .default(1)
                            .comment("执行顺序，越小越先执行"),
                    )
                    .col(
                        ColumnDef::new(RoutePluginBindings::Blocking)
                            .boolean()
                            .not_null()
                            .default(true)
                            .comment("是否阻塞式插件（阻塞式拦截请求/响应，非阻塞仅旁路观测）"),
                    )
                    .col(
                        ColumnDef::new(RoutePluginBindings::Enabled)
                            .boolean()
                            .not_null()
                            .default(true)
                            .comment("是否启用该绑定"),
                    )
                    .col(
                        ColumnDef::new(RoutePluginBindings::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("创建时间"),
                    )
                    .col(
                        ColumnDef::new(RoutePluginBindings::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("更新时间"),
                    )
                    .col(
                        ColumnDef::new(RoutePluginBindings::DeletedAt)
                            .timestamp_with_time_zone()
                            .null()
                            .comment("软删除时间"),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_bindings_route")
                            .from(RoutePluginBindings::Table, RoutePluginBindings::RouteId)
                            .to(
                                super::m20260101_000003_routes::Routes::Table,
                                super::m20260101_000003_routes::Routes::Id,
                            )
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // 路由+插件唯一绑定（活跃行）：同上做方言分支
        if manager.get_database_backend() == DatabaseBackend::MySql {
            manager
                .create_index(
                    Index::create()
                        .name("uk_route_plugin")
                        .table(RoutePluginBindings::Table)
                        .col(RoutePluginBindings::RouteId)
                        .col(RoutePluginBindings::PluginName)
                        .col(RoutePluginBindings::DeletedAt)
                        .unique()
                        .to_owned(),
                )
                .await?;
        } else {
            manager
                .get_connection()
                .execute_unprepared(
                    "CREATE UNIQUE INDEX uk_route_plugin ON route_plugin_bindings (route_id, plugin_name) WHERE deleted_at IS NULL",
                )
                .await?;
        }

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(RoutePluginBindings::Table).to_owned())
            .await?;
        Ok(())
    }
}
