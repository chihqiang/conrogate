//! 迁移：上游节点表。

use sea_orm_migration::prelude::*;
use sea_orm_migration::MigrationName;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260101_000002_upstream_nodes"
    }
}

#[derive(DeriveIden)]
enum UpstreamNodes {
    Table,
    Id,
    UpstreamId,
    Address,
    Weight,
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
                    .table(UpstreamNodes::Table)
                    .col(
                        ColumnDef::new(UpstreamNodes::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key()
                            .comment("节点主键"),
                    )
                    .col(
                        ColumnDef::new(UpstreamNodes::UpstreamId)
                            .big_integer()
                            .not_null()
                            .comment("所属上游组 ID"),
                    )
                    .col(
                        ColumnDef::new(UpstreamNodes::Address)
                            .string()
                            .not_null()
                            .comment("节点地址 host:port（可带 http(s):// scheme）"),
                    )
                    .col(
                        ColumnDef::new(UpstreamNodes::Weight)
                            .integer()
                            .not_null()
                            .default(1)
                            .comment("加权轮询权重"),
                    )
                    .col(
                        ColumnDef::new(UpstreamNodes::Enabled)
                            .boolean()
                            .not_null()
                            .default(true)
                            .comment("是否启用该节点"),
                    )
                    .col(
                        ColumnDef::new(UpstreamNodes::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("创建时间"),
                    )
                    .col(
                        ColumnDef::new(UpstreamNodes::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("更新时间"),
                    )
                    .col(
                        ColumnDef::new(UpstreamNodes::DeletedAt)
                            .timestamp_with_time_zone()
                            .null()
                            .comment("软删除时间"),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_nodes_upstream")
                            .from(UpstreamNodes::Table, UpstreamNodes::UpstreamId)
                            .to(
                                super::m20260101_000001_upstreams::Upstreams::Table,
                                super::m20260101_000001_upstreams::Upstreams::Id,
                            )
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_upstream_nodes_upstream")
                    .table(UpstreamNodes::Table)
                    .col(UpstreamNodes::UpstreamId)
                    .col(UpstreamNodes::Enabled)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(UpstreamNodes::Table).to_owned())
            .await?;
        Ok(())
    }
}
