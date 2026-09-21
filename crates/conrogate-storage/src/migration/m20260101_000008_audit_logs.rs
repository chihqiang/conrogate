//! 迁移：审计日志表。

use sea_orm_migration::prelude::*;
use sea_orm_migration::MigrationName;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260101_000008_audit_logs"
    }
}

#[derive(DeriveIden)]
enum AuditLogs {
    Table,
    Id,
    Ts,
    Operator,
    Action,
    Resource,
    ResourceId,
    Detail,
    TraceId,
    CreatedAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(AuditLogs::Table)
                    .col(
                        ColumnDef::new(AuditLogs::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key()
                            .comment("审计日志主键"),
                    )
                    .col(
                        ColumnDef::new(AuditLogs::Ts)
                            .timestamp_with_time_zone()
                            .not_null()
                            .comment("操作时间"),
                    )
                    .col(
                        ColumnDef::new(AuditLogs::Operator)
                            .string()
                            .null()
                            .comment("操作人"),
                    )
                    .col(
                        ColumnDef::new(AuditLogs::Action)
                            .string()
                            .not_null()
                            .comment("操作动作"),
                    )
                    .col(
                        ColumnDef::new(AuditLogs::Resource)
                            .string()
                            .not_null()
                            .comment("操作资源"),
                    )
                    .col(
                        ColumnDef::new(AuditLogs::ResourceId)
                            .big_integer()
                            .null()
                            .comment("资源 ID"),
                    )
                    .col(
                        ColumnDef::new(AuditLogs::Detail)
                            .json()
                            .null()
                            .comment("操作详情 JSON"),
                    )
                    .col(
                        ColumnDef::new(AuditLogs::TraceId)
                            .string()
                            .null()
                            .comment("追踪 ID"),
                    )
                    .col(
                        ColumnDef::new(AuditLogs::CreatedAt)
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
                    .name("idx_audit_ts")
                    .table(AuditLogs::Table)
                    .col(AuditLogs::Ts)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(AuditLogs::Table).to_owned())
            .await?;
        Ok(())
    }
}
