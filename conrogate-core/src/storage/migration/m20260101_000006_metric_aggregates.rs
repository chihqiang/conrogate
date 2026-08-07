//! 迁移：指标聚合表。

use sea_orm_migration::prelude::*;
use sea_orm_migration::MigrationName;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260101_000006_metric_aggregates"
    }
}

#[derive(DeriveIden)]
enum MetricAggregates {
    Table,
    Id,
    Ts,
    BucketSec,
    RouteId,
    GateId,
    Qps,
    TotalRequests,
    AvgLatencyMs,
    P50Ms,
    P90Ms,
    P99Ms,
    Status2xx,
    Status3xx,
    Status4xx,
    Status5xx,
    Sessions,
    BytesIn,
    BytesOut,
    CreatedAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(MetricAggregates::Table)
                    .col(
                        ColumnDef::new(MetricAggregates::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key()
                            .comment("指标主键"),
                    )
                    .col(
                        ColumnDef::new(MetricAggregates::Ts)
                            .timestamp_with_time_zone()
                            .not_null()
                            .comment("时间桶起点"),
                    )
                    .col(
                        ColumnDef::new(MetricAggregates::BucketSec)
                            .integer()
                            .not_null()
                            .default(10)
                            .comment("时间桶时长（秒）"),
                    )
                    .col(
                        ColumnDef::new(MetricAggregates::RouteId)
                            .big_integer()
                            .null()
                            .comment("路由 ID（NULL=非路由维度汇总）"),
                    )
                    .col(
                        ColumnDef::new(MetricAggregates::GateId)
                            .string()
                            .null()
                            .comment("网关实例 ID"),
                    )
                    .col(
                        ColumnDef::new(MetricAggregates::Qps)
                            .integer()
                            .not_null()
                            .default(0)
                            .comment("桶内统计 QPS"),
                    )
                    .col(
                        ColumnDef::new(MetricAggregates::TotalRequests)
                            .big_integer()
                            .not_null()
                            .default(0)
                            .comment("桶内总请求数"),
                    )
                    .col(
                        ColumnDef::new(MetricAggregates::AvgLatencyMs)
                            .double()
                            .not_null()
                            .default(0.0)
                            .comment("桶内平均延迟（ms）"),
                    )
                    .col(
                        ColumnDef::new(MetricAggregates::P50Ms)
                            .integer()
                            .not_null()
                            .default(0)
                            .comment("P50 延迟（ms）"),
                    )
                    .col(
                        ColumnDef::new(MetricAggregates::P90Ms)
                            .integer()
                            .not_null()
                            .default(0)
                            .comment("P90 延迟（ms）"),
                    )
                    .col(
                        ColumnDef::new(MetricAggregates::P99Ms)
                            .integer()
                            .not_null()
                            .default(0)
                            .comment("P99 延迟（ms）"),
                    )
                    .col(
                        ColumnDef::new(MetricAggregates::Status2xx)
                            .big_integer()
                            .not_null()
                            .default(0)
                            .comment("2xx 响应数"),
                    )
                    .col(
                        ColumnDef::new(MetricAggregates::Status3xx)
                            .big_integer()
                            .not_null()
                            .default(0)
                            .comment("3xx 响应数"),
                    )
                    .col(
                        ColumnDef::new(MetricAggregates::Status4xx)
                            .big_integer()
                            .not_null()
                            .default(0)
                            .comment("4xx 响应数"),
                    )
                    .col(
                        ColumnDef::new(MetricAggregates::Status5xx)
                            .big_integer()
                            .not_null()
                            .default(0)
                            .comment("5xx 响应数"),
                    )
                    .col(
                        ColumnDef::new(MetricAggregates::Sessions)
                            .big_integer()
                            .not_null()
                            .default(0)
                            .comment("WebSocket 会话数"),
                    )
                    .col(
                        ColumnDef::new(MetricAggregates::BytesIn)
                            .big_integer()
                            .not_null()
                            .default(0)
                            .comment("入站字节数（上游→客户端）"),
                    )
                    .col(
                        ColumnDef::new(MetricAggregates::BytesOut)
                            .big_integer()
                            .not_null()
                            .default(0)
                            .comment("出站字节数（客户端→上游）"),
                    )
                    .col(
                        ColumnDef::new(MetricAggregates::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("创建时间"),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("uk_metric_aggregate")
                    .table(MetricAggregates::Table)
                    .col(MetricAggregates::Ts)
                    .col(MetricAggregates::BucketSec)
                    .col(MetricAggregates::RouteId)
                    .col(MetricAggregates::GateId)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_metric_ts")
                    .table(MetricAggregates::Table)
                    .col(MetricAggregates::Ts)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(MetricAggregates::Table).to_owned())
            .await?;
        Ok(())
    }
}
