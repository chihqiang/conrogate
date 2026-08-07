//! 配置缓存与加载器实现。

use crate::contract::dto::ConfigSnapshot;
use crate::contract::storage::{
    ConfigCache, ConfigLoader, ReadOnlyPluginBindingRepo, ReadOnlyRouteRepo, ReadOnlyUpstreamRepo,
};
use crate::contract::ConrogateError;
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
        use sea_orm::ConnectionTrait;
        let stmt = sea_orm::Statement::from_sql_and_values(
            self.db.get_database_backend(),
            "SELECT MAX(version) as v FROM config_versions",
            [],
        );
        let result = self
            .db
            .query_one(stmt)
            .await
            .map_err(|e| ConrogateError::Internal(format!("query version: {e}")))?;
        if let Some(row) = result {
            let v: Option<i64> = sea_orm::TryGetable::try_get(&row, "", "v")
                .map_err(|e| ConrogateError::Internal(format!("get version: {:?}", e)))?;
            Ok(v.map(|v| v as u64))
        } else {
            Ok(None)
        }
    }

    async fn get_snapshot(&self) -> Result<Option<ConfigSnapshot>, ConrogateError> {
        let route_repo =
            crate::storage::repository::route_repo::RouteRepoImpl::new((*self.db).clone());
        let upstream_repo =
            crate::storage::repository::upstream_repo::UpstreamRepoImpl::new((*self.db).clone());

        let routes = ReadOnlyRouteRepo::list_enabled(&route_repo).await?;
        let upstreams = ReadOnlyUpstreamRepo::list_all(&upstream_repo).await?;

        let mut bindings = Vec::new();
        for route in &routes {
            let binding_repo =
                crate::storage::repository::plugin_binding_repo::PluginBindingRepoImpl::new(
                    (*self.db).clone(),
                );
            let route_bindings =
                ReadOnlyPluginBindingRepo::list_by_route(&binding_repo, route.id).await?;
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

    async fn invalidate(&self) -> Result<(), ConrogateError> {
        // DB 模式无缓存可失效
        Ok(())
    }

    async fn subscribe_changes(
        &self,
    ) -> Result<Option<tokio::sync::watch::Receiver<u64>>, ConrogateError> {
        // DB 模式不支持 Pub/Sub，返回 None（降级为轮询）
        Ok(None)
    }
}

/// Redis 配置缓存（默认实现）
pub struct RedisConfigCache {
    redis: Arc<redis::Client>,
    /// watch channel 用于通知版本变更
    notify_tx: tokio::sync::watch::Sender<u64>,
}

impl RedisConfigCache {
    pub fn new(redis_url: &str) -> Result<Self, ConrogateError> {
        let client = redis::Client::open(redis_url)
            .map_err(|e| ConrogateError::Internal(format!("redis connect: {e}")))?;
        let (notify_tx, _) = tokio::sync::watch::channel(0);
        Ok(Self {
            redis: Arc::new(client),
            notify_tx,
        })
    }

    const VERSION_KEY: &'static str = "conrogate:config:version";
    const SNAPSHOT_PREFIX: &'static str = "conrogate:config:snapshot:";
    const NOTIFY_CHANNEL: &'static str = "conrogate:config:notify";
    /// 快照写入失败最大重试次数
    const RETRY_MAX: u32 = 3;
    /// 重试退避间隔（毫秒）
    const RETRY_BACKOFF_MS: std::time::Duration = std::time::Duration::from_millis(200);

    fn snapshot_key(version: u64) -> String {
        format!("{}{}", Self::SNAPSHOT_PREFIX, version)
    }

    /// 单次原子写入版本号 + 快照 + 发布通知
    async fn write_snapshot(&self, version: u64, json: &str) -> Result<(), ConrogateError> {
        let mut conn = self
            .redis
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| ConrogateError::Internal(format!("redis get_connection: {e}")))?;

        let _: () = redis::pipe()
            .atomic()
            .cmd("SET")
            .arg(Self::VERSION_KEY)
            .arg(version)
            .cmd("SET")
            .arg(Self::snapshot_key(version))
            .arg(json)
            .cmd("PUBLISH")
            .arg(Self::NOTIFY_CHANNEL)
            .arg(version)
            .query_async(&mut conn)
            .await
            .map_err(|e| ConrogateError::Internal(format!("redis pipe: {e}")))?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl ConfigCache for RedisConfigCache {
    async fn get_version(&self) -> Result<Option<u64>, ConrogateError> {
        let mut conn = self
            .redis
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| ConrogateError::Internal(format!("redis get_connection: {e}")))?;
        let v: Option<String> = redis::cmd("GET")
            .arg(Self::VERSION_KEY)
            .query_async(&mut conn)
            .await
            .map_err(|e| ConrogateError::Internal(format!("redis GET version: {e}")))?;
        match v {
            Some(s) => {
                let version: u64 = s
                    .parse()
                    .map_err(|e| ConrogateError::Internal(format!("version parse: {e}")))?;
                Ok(Some(version))
            }
            None => Ok(None),
        }
    }

    async fn get_snapshot(&self) -> Result<Option<ConfigSnapshot>, ConrogateError> {
        let version = match self.get_version().await? {
            Some(v) => v,
            None => return Ok(None),
        };

        let mut conn = self
            .redis
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| ConrogateError::Internal(format!("redis get_connection: {e}")))?;
        let json: Option<String> = redis::cmd("GET")
            .arg(Self::snapshot_key(version))
            .query_async(&mut conn)
            .await
            .map_err(|e| ConrogateError::Internal(format!("redis GET snapshot: {e}")))?;

        match json {
            Some(s) => {
                let snap: ConfigSnapshot = serde_json::from_str(&s)
                    .map_err(|e| ConrogateError::Internal(format!("snapshot deserialize: {e}")))?;
                Ok(Some(snap))
            }
            None => Ok(None),
        }
    }

    async fn put_snapshot(
        &self,
        version: u64,
        snapshot: &ConfigSnapshot,
    ) -> Result<(), ConrogateError> {
        let json = serde_json::to_string(snapshot)
            .map_err(|e| ConrogateError::Internal(format!("snapshot serialize: {e}")))?;

        // 写入失败重试（最多 RETRY_MAX 次），降低瞬时抖动导致的发布失败
        let mut last_err: Option<ConrogateError> = None;
        for attempt in 1..=Self::RETRY_MAX {
            match self.write_snapshot(version, &json).await {
                Ok(()) => {
                    if attempt > 1 {
                        tracing::warn!(version, attempt, "redis config cache write retried");
                    }
                    // 通知本地订阅者
                    let _ = self.notify_tx.send(version);
                    return Ok(());
                }
                Err(e) => {
                    last_err = Some(e);
                    if attempt < Self::RETRY_MAX {
                        tokio::time::sleep(Self::RETRY_BACKOFF_MS).await;
                    }
                }
            }
        }

        Err(last_err
            .unwrap_or_else(|| ConrogateError::Internal("redis config cache write failed".into())))
    }

    async fn invalidate(&self) -> Result<(), ConrogateError> {
        let mut conn = self
            .redis
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| ConrogateError::Internal(format!("redis get_connection: {e}")))?;

        // 删除版本号，使数据面 get_snapshot 返回 None → 降级直连 DB 轮询
        let _: () = redis::cmd("DEL")
            .arg(Self::VERSION_KEY)
            .query_async(&mut conn)
            .await
            .map_err(|e| ConrogateError::Internal(format!("redis DEL version: {e}")))?;
        Ok(())
    }

    async fn subscribe_changes(
        &self,
    ) -> Result<Option<tokio::sync::watch::Receiver<u64>>, ConrogateError> {
        Ok(Some(self.notify_tx.subscribe()))
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
        self.cache
            .get_snapshot()
            .await?
            .ok_or_else(|| ConrogateError::Internal("config snapshot not available".into()))
    }

    async fn current_version(&self) -> Result<u64, ConrogateError> {
        self.cache.get_version().await.map(|v| v.unwrap_or(0))
    }
}
