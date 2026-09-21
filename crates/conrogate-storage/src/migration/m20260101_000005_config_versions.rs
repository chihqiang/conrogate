//! 迁移：配置版本表。

use sea_orm_migration::prelude::*;
use sea_orm_migration::MigrationName;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260101_000005_config_versions"
    }
}

#[derive(DeriveIden)]
enum ConfigVersions {
    Table,
    Id,
    Version,
    BaseVersion,
    PublishType,
    ContentHash,
    SnapshotContent,
    CreatedBy,
    Remark,
    CreatedAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(ConfigVersions::Table)
                    .col(
                        ColumnDef::new(ConfigVersions::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key()
                            .comment("版本记录主键"),
                    )
                    .col(
                        ColumnDef::new(ConfigVersions::Version)
                            .big_integer()
                            .not_null()
                            .comment("配置版本号（单调递增）"),
                    )
                    .col(
                        ColumnDef::new(ConfigVersions::BaseVersion)
                            .big_integer()
                            .null()
                            .comment("基础版本号（回滚前的版本）"),
                    )
                    .col(
                        ColumnDef::new(ConfigVersions::PublishType)
                            .small_integer()
                            .not_null()
                            .default(0)
                            .comment("发布类型：0=发布 1=回滚"),
                    )
                    .col(
                        ColumnDef::new(ConfigVersions::ContentHash)
                            .string()
                            .not_null()
                            .comment("快照内容哈希（内容一致性校验）"),
                    )
                    .col(
                        ColumnDef::new(ConfigVersions::SnapshotContent)
                            .json()
                            .not_null()
                            .comment("配置快照 JSON"),
                    )
                    .col(
                        ColumnDef::new(ConfigVersions::CreatedBy)
                            .string()
                            .null()
                            .comment("创建人"),
                    )
                    .col(
                        ColumnDef::new(ConfigVersions::Remark)
                            .string()
                            .null()
                            .comment("发布备注"),
                    )
                    .col(
                        ColumnDef::new(ConfigVersions::CreatedAt)
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
                    .name("idx_config_versions_version")
                    .table(ConfigVersions::Table)
                    .col(ConfigVersions::Version)
                    .unique()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(ConfigVersions::Table).to_owned())
            .await?;
        Ok(())
    }
}
