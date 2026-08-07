//! 迁移：上游组表。

use sea_orm_migration::prelude::*;
use sea_orm_migration::MigrationName;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260101_000001_upstreams"
    }
}

#[derive(DeriveIden)]
pub enum Upstreams {
    Table,
    Id,
    Name,
    Algorithm,
    RetryEnabled,
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
                    .table(Upstreams::Table)
                    .col(
                        ColumnDef::new(Upstreams::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key()
                            .comment("上游组主键"),
                    )
                    .col(
                        ColumnDef::new(Upstreams::Name)
                            .string()
                            .not_null()
                            .comment("上游组名称"),
                    )
                    .col(
                        ColumnDef::new(Upstreams::Algorithm)
                            .small_integer()
                            .not_null()
                            .default(1)
                            .comment("负载均衡算法：1=round_robin 2=weighted_round_robin 3=least_connections 4=consistent_hash"),
                    )
                    .col(
                        ColumnDef::new(Upstreams::RetryEnabled)
                            .boolean()
                            .not_null()
                            .default(true)
                            .comment("是否启用失败自动重试"),
                    )
                    .col(
                        ColumnDef::new(Upstreams::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("创建时间"),
                    )
                    .col(
                        ColumnDef::new(Upstreams::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("更新时间"),
                    )
                    .col(
                        ColumnDef::new(Upstreams::DeletedAt)
                            .timestamp_with_time_zone()
                            .null()
                            .comment("软删除时间"),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_upstreams_name")
                    .table(Upstreams::Table)
                    .col(Upstreams::Name)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Upstreams::Table).to_owned())
            .await?;
        Ok(())
    }
}
