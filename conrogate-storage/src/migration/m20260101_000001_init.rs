//! 初始迁移：创建全部表 + 索引。

use sea_orm::DatabaseBackend;
use sea_orm_migration::prelude::*;
use sea_orm_migration::MigrationName;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260101_000001_init"
    }
}

#[derive(DeriveIden)]
enum Routes {
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

#[derive(DeriveIden)]
enum Upstreams {
    Table,
    Id,
    Name,
    Algorithm,
    RetryEnabled,
    CreatedAt,
    UpdatedAt,
    DeletedAt,
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

#[derive(DeriveIden)]
enum RoutePluginBindings {
    Table,
    Id,
    RouteId,
    PluginName,
    Config,
    #[sea_orm(iden = "order")]
    Order,
    Blocking,
    Enabled,
    CreatedAt,
    UpdatedAt,
    DeletedAt,
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
    #[sea_orm(iden = "status_2xx")]
    Status2xx,
    #[sea_orm(iden = "status_3xx")]
    Status3xx,
    #[sea_orm(iden = "status_4xx")]
    Status4xx,
    #[sea_orm(iden = "status_5xx")]
    Status5xx,
    Sessions,
    BytesIn,
    BytesOut,
    CreatedAt,
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

#[derive(DeriveIden)]
enum NodeApplications {
    Table,
    Id,
    GateId,
    Version,
    AppliedAt,
    UpdatedAt,
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
        // ── 1. upstreams ──
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

        // ── 2. upstream_nodes ──
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
                            .to(Upstreams::Table, Upstreams::Id)
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

        // ── 3. routes ──
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
                            .to(Upstreams::Table, Upstreams::Id)
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

        // ── 4. route_plugin_bindings ──
        manager
            .create_table(
                Table::create()
                    .table(RoutePluginBindings::Table)
                    .col(
                        ColumnDef::new(RoutePluginBindings::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key()
                            .comment("绑定主键"),
                    )
                    .col(
                        ColumnDef::new(RoutePluginBindings::RouteId)
                            .big_integer()
                            .not_null()
                            .comment("路由 ID"),
                    )
                    .col(
                        ColumnDef::new(RoutePluginBindings::PluginName)
                            .string()
                            .not_null()
                            .comment("插件名"),
                    )
                    .col(
                        ColumnDef::new(RoutePluginBindings::Config)
                            .json()
                            .not_null()
                            .comment("插件配置 JSON（无 DB 默认值：MySQL JSON 列不支持默认值，由应用层写入）"),
                    )
                    .col(
                        ColumnDef::new(RoutePluginBindings::Order)
                            .integer()
                            .not_null()
                            .default(1)
                            .comment("执行顺序，越小越先执行"),
                    )
                    .col(
                        ColumnDef::new(RoutePluginBindings::Blocking)
                            .boolean()
                            .not_null()
                            .default(true)
                            .comment("是否阻塞式插件（阻塞式拦截请求/响应，非阻塞仅旁路观测）"),
                    )
                    .col(
                        ColumnDef::new(RoutePluginBindings::Enabled)
                            .boolean()
                            .not_null()
                            .default(true)
                            .comment("是否启用该绑定"),
                    )
                    .col(
                        ColumnDef::new(RoutePluginBindings::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("创建时间"),
                    )
                    .col(
                        ColumnDef::new(RoutePluginBindings::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp())
                            .comment("更新时间"),
                    )
                    .col(
                        ColumnDef::new(RoutePluginBindings::DeletedAt)
                            .timestamp_with_time_zone()
                            .null()
                            .comment("软删除时间"),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_bindings_route")
                            .from(RoutePluginBindings::Table, RoutePluginBindings::RouteId)
                            .to(Routes::Table, Routes::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // 路由+插件唯一绑定（活跃行）：同上做方言分支
        if manager.get_database_backend() == DatabaseBackend::MySql {
            manager
                .create_index(
                    Index::create()
                        .name("uk_route_plugin")
                        .table(RoutePluginBindings::Table)
                        .col(RoutePluginBindings::RouteId)
                        .col(RoutePluginBindings::PluginName)
                        .col(RoutePluginBindings::DeletedAt)
                        .unique()
                        .to_owned(),
                )
                .await?;
        } else {
            manager
                .get_connection()
                .execute_unprepared(
                    "CREATE UNIQUE INDEX uk_route_plugin ON route_plugin_bindings (route_id, plugin_name) WHERE deleted_at IS NULL",
                )
                .await?;
        }

        // ── 5. config_versions ──
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

        // ── 6. metric_aggregates ──
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

        // ── 7. gateway_events ──
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
                            .comment("事件类型（rate_limited/circuit_breaker_open/upstream_failed 等）"),
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

        // ── 8. audit_logs ──
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

        // ── 9. node_applications ──
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

        // ── 10. installed_plugins ──
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
        // 按依赖逆序删除
        let tables = [
            "installed_plugins",
            "node_applications",
            "audit_logs",
            "gateway_events",
            "metric_aggregates",
            "config_versions",
            "route_plugin_bindings",
            "routes",
            "upstream_nodes",
            "upstreams",
        ];
        for table in tables {
            manager
                .drop_table(Table::drop().table(Alias::new(table)).to_owned())
                .await?;
        }
        Ok(())
    }
}
