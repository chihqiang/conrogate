//! 迁移：网关事件表。

use sea_orm_migration::prelude::*;
use sea_orm_migration::MigrationName;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260101_000007_gateway_events"
    }
}

#[derive(DeriveIden)]
enum GatewayEvents {
    Table,
    Id,
    Ts,
    EventType,
    RouteId,
    UpstreamId,
    TraceId,
    Detail,
    CreatedAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(GatewayEvents::Table)
                    .col(
                        ColumnDef::new(GatewayEvents::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key()
                            .comment("事件主键"),
                    )
                    .col(
                        ColumnDef::new(GatewayEvents::Ts)
                            .timestamp_with_time_zone()
                            .not_null()
                            .comment("事件发生时间"),
                    )
                    .col(
                        ColumnDef::new(GatewayEvents::EventType)
                            .string()
                            .not_null()
                            .comment(
                                "事件类型（rate_limited/circuit_breaker_open/upstream_failed 等）",
                            ),
                    )
                    .col(
                        ColumnDef::new(GatewayEvents::RouteId)
                            .big_integer()
                            .null()
                            .comment("相关路由 ID"),
                    )
                    .col(
                        ColumnDef::new(GatewayEvents::UpstreamId)
                            .big_integer()
                            .null()
                            .comment("相关上游组 ID"),
                    )
                    .col(
                        ColumnDef::new(GatewayEvents::TraceId)
                            .string()
                            .null()
                            .comment("追踪 ID（参与幂等去重）"),
                    )
                    .col(
                        ColumnDef::new(GatewayEvents::Detail)
                            .json()
                            .null()
                            .comment("事件详情 JSON"),
                    )
                    .col(
                        ColumnDef::new(GatewayEvents::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("入库时间"),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_events_type_ts")
                    .table(GatewayEvents::Table)
                    .col(GatewayEvents::EventType)
                    .col(GatewayEvents::Ts)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("uk_event_dedup")
                    .table(GatewayEvents::Table)
                    .col(GatewayEvents::TraceId)
                    .col(GatewayEvents::Ts)
                    .col(GatewayEvents::EventType)
                    .unique()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(GatewayEvents::Table).to_owned())
            .await?;
        Ok(())
    }
}
