//! metric_aggregates 表实体。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "metric_aggregates")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub ts: DateTimeUtc,
    #[sea_orm(column_name = "bucket_sec")]
    pub bucket_sec: i32,
    #[sea_orm(column_name = "route_id", nullable)]
    pub route_id: Option<i64>,
    #[sea_orm(column_name = "gate_id")]
    pub gate_id: String,
    pub qps: i32,
    #[sea_orm(column_name = "total_requests")]
    pub total_requests: i64,
    #[sea_orm(column_name = "avg_latency_ms")]
    pub avg_latency_ms: f64,
    #[sea_orm(column_name = "p50_ms")]
    pub p50_ms: i32,
    #[sea_orm(column_name = "p90_ms")]
    pub p90_ms: i32,
    #[sea_orm(column_name = "p99_ms")]
    pub p99_ms: i32,
    #[sea_orm(column_name = "status_2xx")]
    pub status_2xx: i64,
    #[sea_orm(column_name = "status_3xx")]
    pub status_3xx: i64,
    #[sea_orm(column_name = "status_4xx")]
    pub status_4xx: i64,
    #[sea_orm(column_name = "status_5xx")]
    pub status_5xx: i64,
    pub sessions: i64,
    #[sea_orm(column_name = "bytes_in")]
    pub bytes_in: i64,
    #[sea_orm(column_name = "bytes_out")]
    pub bytes_out: i64,
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
