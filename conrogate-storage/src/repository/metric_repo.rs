//! 指标仓储实现。

use crate::convert;
use crate::entity::metric_aggregates::{self, Entity as MetricEntity};
use conrogate_contract::dto::{MetricQuery, MetricRow, OverviewMetric};
use conrogate_contract::storage::MetricRepo;
use conrogate_contract::ConrogateError;
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder,
};

pub struct MetricRepoImpl {
    db: DatabaseConnection,
}

impl MetricRepoImpl {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl MetricRepo for MetricRepoImpl {
    async fn upsert_batch(&self, metrics: &[MetricRow]) -> Result<(), ConrogateError> {
        if metrics.is_empty() {
            return Ok(());
        }
        // 逐条 upsert（PostgreSQL ON CONFLICT）
        for row in metrics {
            let active = convert::metric_row_to_active_model(row);
            // 先尝试查找已有记录
            let existing = MetricEntity::find()
                .filter(metric_aggregates::Column::Ts.eq(row.ts))
                .filter(metric_aggregates::Column::BucketSec.eq(row.bucket_sec as i32))
                .filter(metric_aggregates::Column::GateId.eq(&row.gate_id))
                .filter(metric_aggregates::Column::RouteId.eq(row.route_id.map(|v| v as i64)))
                .one(&self.db)
                .await
                .map_err(|_| ConrogateError::DatabaseInternal)?;

            if let Some(_existing) = existing {
                // 更新已存在的桶
                MetricEntity::update_many()
                    .col_expr(metric_aggregates::Column::Qps, Expr::value(row.qps as i32))
                    .col_expr(
                        metric_aggregates::Column::TotalRequests,
                        Expr::value(row.total_requests as i64),
                    )
                    .col_expr(
                        metric_aggregates::Column::AvgLatencyMs,
                        Expr::value(row.avg_latency_ms),
                    )
                    .col_expr(
                        metric_aggregates::Column::Status2xx,
                        Expr::value(row.status_2xx as i64),
                    )
                    .col_expr(
                        metric_aggregates::Column::Status4xx,
                        Expr::value(row.status_4xx as i64),
                    )
                    .col_expr(
                        metric_aggregates::Column::Status5xx,
                        Expr::value(row.status_5xx as i64),
                    )
                    .filter(metric_aggregates::Column::Ts.eq(row.ts))
                    .filter(metric_aggregates::Column::BucketSec.eq(row.bucket_sec as i32))
                    .filter(metric_aggregates::Column::GateId.eq(&row.gate_id))
                    .filter(metric_aggregates::Column::RouteId.eq(row.route_id.map(|v| v as i64)))
                    .exec(&self.db)
                    .await
                    .map_err(|_| ConrogateError::DatabaseInternal)?;
            } else {
                // 插入新桶
                active
                    .insert(&self.db)
                    .await
                    .map_err(|e| ConrogateError::DataMapping(e.to_string()))?;
            }
        }
        Ok(())
    }

    async fn query(&self, filter: &MetricQuery) -> Result<Vec<MetricRow>, ConrogateError> {
        let mut query = MetricEntity::find()
            .filter(
                metric_aggregates::Column::Ts
                    .gte(chrono::Utc::now() - chrono::Duration::minutes(filter.range_min as i64)),
            )
            .order_by_asc(metric_aggregates::Column::Ts);

        if let Some(route_id) = filter.route_id {
            query = query.filter(metric_aggregates::Column::RouteId.eq(route_id as i64));
        }
        if let Some(ref gate_id) = filter.gate_id {
            query = query.filter(metric_aggregates::Column::GateId.eq(gate_id));
        }

        let models = query
            .all(&self.db)
            .await
            .map_err(|_| ConrogateError::DatabaseInternal)?;

        Ok(models
            .into_iter()
            .filter_map(convert::metric_model_to_row)
            .collect())
    }

    async fn overview(&self, range_min: u32) -> Result<OverviewMetric, ConrogateError> {
        let since = chrono::Utc::now() - chrono::Duration::minutes(range_min as i64);
        let models = MetricEntity::find()
            .filter(metric_aggregates::Column::Ts.gte(since))
            .all(&self.db)
            .await
            .map_err(|_| ConrogateError::DatabaseInternal)?;

        if models.is_empty() {
            return Ok(OverviewMetric {
                total_qps: 0.0,
                avg_latency_ms: 0.0,
                error_rate: 0.0,
            });
        }

        let total_requests: i64 = models.iter().map(|m| m.total_requests).sum();
        let total_errors: i64 = models.iter().map(|m| m.status_4xx + m.status_5xx).sum();
        let total_latency: f64 = models
            .iter()
            .map(|m| m.avg_latency_ms * m.total_requests as f64)
            .sum();

        let seconds = range_min * 60;
        let total_qps = if seconds > 0 {
            total_requests as f64 / seconds as f64
        } else {
            0.0
        };
        let avg_latency = if total_requests > 0 {
            total_latency / total_requests as f64
        } else {
            0.0
        };
        let error_rate = if total_requests > 0 {
            total_errors as f64 / total_requests as f64
        } else {
            0.0
        };

        Ok(OverviewMetric {
            total_qps,
            avg_latency_ms: avg_latency,
            error_rate,
        })
    }
}
