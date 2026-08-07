//! 迁移：路由表。

use sea_orm::DatabaseBackend;
use sea_orm_migration::prelude::*;
use sea_orm_migration::MigrationName;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260101_000003_routes"
    }
}

#[derive(DeriveIden)]
pub enum Routes {
    Table,
    Id,
    Name,
    Protocol,
    MatchConditions,
    Priority,
    UpstreamId,
    HostHeader,
    AllowRetryNonIdempotent,
    WsStripSensitiveHeaders,
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
                    .table(Routes::Table)
                    .col(
                        ColumnDef::new(Routes::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key()
                            .comment("路由主键"),
                    )
                    .col(
                        ColumnDef::new(Routes::Name)
                            .string()
                            .not_null()
                            .comment("路由名称"),
                    )
                    .col(
                        ColumnDef::new(Routes::Protocol)
                            .small_integer()
                            .not_null()
                            .default(1)
                            .comment("协议：1=http 2=websocket 3=tcp_tunnel"),
                    )
                    .col(
                        ColumnDef::new(Routes::MatchConditions)
                            .json()
                            .not_null()
                            .comment("匹配条件 JSON（path/methods/host/headers/query_params）"),
                    )
                    .col(
                        ColumnDef::new(Routes::Priority)
                            .integer()
                            .not_null()
                            .default(10)
                            .comment("匹配优先级，越大越先匹配"),
                    )
                    .col(
                        ColumnDef::new(Routes::UpstreamId)
                            .big_integer()
                            .null()
                            .comment("绑定的上游组 ID"),
                    )
                    .col(
                        ColumnDef::new(Routes::HostHeader)
                            .string()
                            .null()
                            .comment("转发时覆盖的 Host 头（缺省用节点地址）"),
                    )
                    .col(
                        ColumnDef::new(Routes::AllowRetryNonIdempotent)
                            .boolean()
                            .not_null()
                            .default(false)
                            .comment("允许重试非幂等请求（POST/PUT 等）"),
                    )
                    .col(
                        ColumnDef::new(Routes::WsStripSensitiveHeaders)
                            .boolean()
                            .not_null()
                            .default(false)
                            .comment("WS 隧道转发上游时是否剥离敏感头（authorization/cookie 等）"),
                    )
                    .col(
                        ColumnDef::new(Routes::Enabled)
                            .boolean()
                            .not_null()
                            .default(true)
                            .comment("是否启用该路由"),
                    )
                    .col(
                        ColumnDef::new(Routes::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("创建时间"),
                    )
                    .col(
                        ColumnDef::new(Routes::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("更新时间"),
                    )
                    .col(
                        ColumnDef::new(Routes::DeletedAt)
                            .timestamp_with_time_zone()
                            .null()
                            .comment("软删除时间"),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_routes_upstream")
                            .from(Routes::Table, Routes::UpstreamId)
                            .to(
                                super::m20260101_000001_upstreams::Upstreams::Table,
                                super::m20260101_000001_upstreams::Upstreams::Id,
                            )
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;

        // 路由名唯一约束（活跃行）：PG/SQLite 支持 partial index；
        // MySQL 不支持谓词索引，退化为 (name, deleted_at) 复合唯一索引，
        // 活跃名唯一性由仓储层预检查保证（见 route_repo）。
        let backend = manager.get_database_backend();
        if backend == DatabaseBackend::MySql {
            manager
                .create_index(
                    Index::create()
                        .name("idx_routes_name")
                        .table(Routes::Table)
                        .col(Routes::Name)
                        .col(Routes::DeletedAt)
                        .unique()
                        .to_owned(),
                )
                .await?;
        } else {
            manager
                .get_connection()
                .execute_unprepared(
                    "CREATE UNIQUE INDEX idx_routes_name ON routes (name) WHERE deleted_at IS NULL",
                )
                .await?;
        }

        manager
            .create_index(
                Index::create()
                    .name("idx_routes_protocol_enabled")
                    .table(Routes::Table)
                    .col(Routes::Protocol)
                    .col(Routes::Enabled)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Routes::Table).to_owned())
            .await?;
        Ok(())
    }
}
