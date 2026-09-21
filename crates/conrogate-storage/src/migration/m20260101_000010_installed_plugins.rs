//! 迁移：已安装插件表。

use sea_orm_migration::prelude::*;
use sea_orm_migration::MigrationName;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260101_000010_installed_plugins"
    }
}

#[derive(DeriveIden)]
enum InstalledPlugins {
    Table,
    Id,
    Name,
    Version,
    ApiVersion,
    Kind,
    Status,
    PackageHash,
    Manifest,
    InstalledAt,
    ActivatedAt,
    DeletedAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(InstalledPlugins::Table)
                    .col(
                        ColumnDef::new(InstalledPlugins::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key()
                            .comment("插件主键"),
                    )
                    .col(
                        ColumnDef::new(InstalledPlugins::Name)
                            .string()
                            .not_null()
                            .comment("插件名"),
                    )
                    .col(
                        ColumnDef::new(InstalledPlugins::Version)
                            .string()
                            .not_null()
                            .comment("插件版本"),
                    )
                    .col(
                        ColumnDef::new(InstalledPlugins::ApiVersion)
                            .integer()
                            .not_null()
                            .default(1)
                            .comment("插件 API 版本"),
                    )
                    .col(
                        ColumnDef::new(InstalledPlugins::Kind)
                            .small_integer()
                            .not_null()
                            .default(0)
                            .comment("插件类型：0=Native 1=Wasm"),
                    )
                    .col(
                        ColumnDef::new(InstalledPlugins::Status)
                            .small_integer()
                            .not_null()
                            .default(0)
                            .comment("插件状态：0=Installed 1=Active 2=Disabled 3=Uninstalled"),
                    )
                    .col(
                        ColumnDef::new(InstalledPlugins::PackageHash)
                            .string()
                            .null()
                            .comment("插件包哈希（完整性校验）"),
                    )
                    .col(
                        ColumnDef::new(InstalledPlugins::Manifest)
                            .json()
                            .not_null()
                            .comment("插件清单 JSON（无 DB 默认值：MySQL JSON 列不支持默认值，由应用层写入）"),
                    )
                    .col(
                        ColumnDef::new(InstalledPlugins::InstalledAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("安装时间"),
                    )
                    .col(
                        ColumnDef::new(InstalledPlugins::ActivatedAt)
                            .timestamp_with_time_zone()
                            .null()
                            .comment("激活时间"),
                    )
                    .col(
                        ColumnDef::new(InstalledPlugins::DeletedAt)
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
                    .name("idx_plugins_name")
                    .table(InstalledPlugins::Table)
                    .col(InstalledPlugins::Name)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(InstalledPlugins::Table).to_owned())
            .await?;
        Ok(())
    }
}
