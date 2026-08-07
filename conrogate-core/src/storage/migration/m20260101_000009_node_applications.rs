//! 迁移：节点应用记录表。

use sea_orm_migration::prelude::*;
use sea_orm_migration::MigrationName;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260101_000009_node_applications"
    }
}

#[derive(DeriveIden)]
enum NodeApplications {
    Table,
    Id,
    GateId,
    Version,
    AppliedAt,
    LastSeen,
    UpdatedAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(NodeApplications::Table)
                    .col(
                        ColumnDef::new(NodeApplications::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key()
                            .comment("应用记录主键"),
                    )
                    .col(
                        ColumnDef::new(NodeApplications::GateId)
                            .string()
                            .not_null()
                            .comment("网关实例 ID"),
                    )
                    .col(
                        ColumnDef::new(NodeApplications::Version)
                            .big_integer()
                            .not_null()
                            .default(0)
                            .comment("已应用的配置版本号"),
                    )
                    .col(
                        ColumnDef::new(NodeApplications::AppliedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("配置应用时间"),
                    )
                    .col(
                        ColumnDef::new(NodeApplications::LastSeen)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("最近心跳时间"),
                    )
                    .col(
                        ColumnDef::new(NodeApplications::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("更新时间"),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_node_apps_gate_id")
                    .table(NodeApplications::Table)
                    .col(NodeApplications::GateId)
                    .unique()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(NodeApplications::Table).to_owned())
            .await?;
        Ok(())
    }
}
