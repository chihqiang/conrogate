//! 配置缓存与加载器实现。

use conrogate_contract::dto::ConfigSnapshot;
use conrogate_contract::storage::{ConfigCache, ConfigLoader, ReadOnlyRouteRepo, ReadOnlyUpstreamRepo, ReadOnlyPluginBindingRepo};
use conrogate_contract::ConrogateError;
use sea_orm::DatabaseConnection;
use std::sync::Arc;

/// 数据库直读配置缓存（无 Redis 时的降级实现）
pub struct DbConfigCache {
    db: Arc<DatabaseConnection>,
}

impl DbConfigCache {
    pub fn new(db: Arc<DatabaseConnection>) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl ConfigCache for DbConfigCache {
    async fn get_version(&self) -> Result<Option<u64>, ConrogateError> {
        // 从 config_versions 表读最新版本号
        // 简化：返回 None 表示需要加载
        Ok(None)
    }

    async fn get_snapshot(&self) -> Result<Option<ConfigSnapshot>, ConrogateError> {
        // 从数据库加载全量配置
        let route_repo = crate::repository::route_repo::RouteRepoImpl::new((*self.db).clone());
        let upstream_repo = crate::repository::upstream_repo::UpstreamRepoImpl::new((*self.db).clone());

        let routes = conrogate_contract::storage::ReadOnlyRouteRepo::list_enabled(&route_repo).await?;
        let upstreams = conrogate_contract::storage::ReadOnlyUpstreamRepo::list_all(&upstream_repo).await?;

        let mut bindings = Vec::new();
        for route in &routes {
            let binding_repo = crate::repository::plugin_binding_repo::PluginBindingRepoImpl::new((*self.db).clone());
            let route_bindings = conrogate_contract::storage::ReadOnlyPluginBindingRepo::list_by_route(&binding_repo, route.id).await?;
            bindings.extend(route_bindings);
        }

        Ok(Some(ConfigSnapshot {
            routes,
            upstreams,
            plugin_bindings: bindings,
        }))
    }

    async fn put_snapshot(
        &self,
        _version: u64,
        _snapshot: &ConfigSnapshot,
    ) -> Result<(), ConrogateError> {
        // DB 模式不需要写缓存（control 直接写库）
        Ok(())
    }

    async fn subscribe_changes(&self) -> Result<Option<tokio::sync::watch::Receiver<u64>>, ConrogateError> {
        // DB 模式不支持 Pub/Sub，返回 None（降级为轮询）
        Ok(None)
    }
}

/// 配置加载器：优先从 ConfigCache 读取，降级直连 DB
pub struct ConfigLoaderImpl {
    cache: Arc<dyn ConfigCache>,
}

impl ConfigLoaderImpl {
    pub fn new(cache: Arc<dyn ConfigCache>) -> Self {
        Self { cache }
    }
}

#[async_trait::async_trait]
impl ConfigLoader for ConfigLoaderImpl {
    async fn load_snapshot(&self) -> Result<ConfigSnapshot, ConrogateError> {
        // 优先从缓存读取
        if let Some(snap) = self.cache.get_snapshot().await? {
            return Ok(snap);
        }
        // 缓存未命中 → 直接从 DB 读取
        self.cache.get_snapshot().await?
            .ok_or_else(|| ConrogateError::Internal("config snapshot not available".into()))
    }

    async fn current_version(&self) -> Result<u64, ConrogateError> {
        self.cache.get_version().await
            .map(|v| v.unwrap_or(0))
    }
}
