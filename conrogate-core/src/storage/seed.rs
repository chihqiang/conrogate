//! 演示数据（mock data）写入。
//!
//! 由 `conrogate-migrate` 在迁移完成后调用；服务二进制
//! （conrogate / conrogate-control / conrogate-gate）不负责数据迁移与演示数据写入。

use sea_orm::DatabaseConnection;

use crate::contract::balancer::BalancerAlgorithm;
use crate::contract::dto::*;
use crate::contract::protocol::{PathMatch, ProtocolId, RouteMatchConditions};
use crate::contract::storage::*;
use crate::contract::ConrogateError;

/// 写入演示数据：1 个上游 + 1 条演示路由（已有数据则跳过）。
pub async fn seed_demo_data(
    main_db: &DatabaseConnection,
    upstream_name: &str,
    upstream_address: &str,
) -> Result<(), ConrogateError> {
    let upstream_repo =
        crate::storage::repository::upstream_repo::UpstreamRepoImpl::new(main_db.clone());
    let route_repo =
        crate::storage::repository::route_repo::RouteRepoImpl::new(main_db.clone());

    // 检查是否已有数据
    let existing = ReadOnlyUpstreamRepo::list_all(&upstream_repo).await?;
    if !existing.is_empty() {
        tracing::info!("demo data already exists, skipping seed");
        return Ok(());
    }

    // 创建上游（指向演示后端 upstream_address）
    let upstream = upstream_repo
        .create(CreateUpstreamDto {
            name: upstream_name.into(),
            algorithm: BalancerAlgorithm::RoundRobin,
            retry_enabled: Some(false),
            nodes: vec![CreateUpstreamNodeDto {
                address: upstream_address.into(),
                weight: Some(1),
                enabled: Some(true),
            }],
        })
        .await?;

    // 创建演示路由
    let _route = route_repo
        .create(CreateRouteDto {
            name: "demo-route".into(),
            protocol: ProtocolId::Http,
            match_conditions: RouteMatchConditions {
                path: PathMatch::Prefix("/demo/".into()),
                methods: None,
                host: None,
                headers: vec![],
                query_params: vec![],
            },
            priority: Some(10),
            upstream_id: Some(upstream.id),
            host_header: None,
            allow_retry_non_idempotent: Some(false),
            ws_strip_sensitive_headers: Some(false),
            enabled: Some(true),
        })
        .await?;

    tracing::info!(
        upstream_id = upstream.id,
        upstream_name,
        "demo data seeded: {} + demo-route",
        upstream_name
    );
    Ok(())
}
