//! 迁移：全局 IP 黑名单表。

use sea_orm::DatabaseBackend;
use sea_orm_migration::prelude::*;
use sea_orm_migration::MigrationName;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260101_000011_ip_blacklist"
    }
}

#[derive(DeriveIden)]
enum IpBlacklist {
    Table,
    Id,
    IpOrCidr,
    Reason,
    ExpiresAt,
    CreatedBy,
    CreatedAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(IpBlacklist::Table)
                    .col(
                        ColumnDef::new(IpBlacklist::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key()
                            .comment("黑名单主键"),
                    )
                    .col(
                        ColumnDef::new(IpBlacklist::IpOrCidr)
                            .string()
                            .not_null()
                            .comment("IP 或 CIDR 网段（IPv4/IPv6），如 1.2.3.4、192.168.0.0/24"),
                    )
                    .col(
                        ColumnDef::new(IpBlacklist::Reason)
                            .string()
                            .null()
                            .comment("拉黑原因"),
                    )
                    .col(
                        ColumnDef::new(IpBlacklist::ExpiresAt)
                            .timestamp_with_time_zone()
                            .null()
                            .comment("过期时间（NULL=永久）"),
                    )
                    .col(
                        ColumnDef::new(IpBlacklist::CreatedBy)
                            .string()
                            .null()
                            .comment("操作人"),
                    )
                    .col(
                        ColumnDef::new(IpBlacklist::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("创建时间"),
                    )
                    .to_owned(),
            )
            .await?;

        // ip_or_cidr 唯一（永久/有效条目直接冲突拒绝）
        let backend = manager.get_database_backend();
        if backend == DatabaseBackend::MySql {
            manager
                .create_index(
                    Index::create()
                        .name("uk_ip_blacklist_ip")
                        .table(IpBlacklist::Table)
                        .col(IpBlacklist::IpOrCidr)
                        .unique()
                        .to_owned(),
                )
                .await?;
        } else {
            manager
                .get_connection()
                .execute_unprepared(
                    "CREATE UNIQUE INDEX uk_ip_blacklist_ip ON ip_blacklist (ip_or_cidr)",
                )
                .await?;
        }

        manager
            .create_index(
                Index::create()
                    .name("idx_ip_blacklist_expires")
                    .table(IpBlacklist::Table)
                    .col(IpBlacklist::ExpiresAt)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(IpBlacklist::Table).to_owned())
            .await?;
        Ok(())
    }
}
