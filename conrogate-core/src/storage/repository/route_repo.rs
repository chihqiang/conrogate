//! 路由仓储实现。

use crate::contract::dto::{CreateRouteDto, PaginatedResult, RouteDto, UpdateRouteDto};
use crate::contract::storage::{ReadOnlyRouteRepo, RouteRepo};
use crate::contract::ConrogateError;
use crate::storage::convert;
use crate::storage::entity::routes::{self, Entity as RouteEntity};
use sea_orm::{
    sea_query::Expr, ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set,
};

pub struct RouteRepoImpl {
    db: DatabaseConnection,
}

impl RouteRepoImpl {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl ReadOnlyRouteRepo for RouteRepoImpl {
    async fn list_enabled(&self) -> Result<Vec<RouteDto>, ConrogateError> {
        let models = RouteEntity::find()
            .filter(routes::Column::Enabled.eq(true))
            .filter(routes::Column::DeletedAt.is_null())
            .order_by_desc(routes::Column::Priority)
            .all(&self.db)
            .await
            .map_err(|_| ConrogateError::DatabaseInternal)?;

        Ok(models
            .into_iter()
            .filter_map(convert::route_model_to_dto)
            .collect())
    }

    async fn find_by_id(&self, id: u64) -> Result<Option<RouteDto>, ConrogateError> {
        let model = RouteEntity::find_by_id(id as i64)
            .filter(routes::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .map_err(|_| ConrogateError::DatabaseInternal)?;

        Ok(model.and_then(convert::route_model_to_dto))
    }
}

#[async_trait::async_trait]
impl RouteRepo for RouteRepoImpl {
    async fn create(&self, dto: CreateRouteDto) -> Result<RouteDto, ConrogateError> {
        // 路由名唯一性预检查（活跃行）。PG/SQLite 有 partial unique index 兜底，
        // MySQL 唯一索引为复合 (name, deleted_at) 无法拦截同名活跃行，故统一在此拦截。
        let dup = RouteEntity::find()
            .filter(routes::Column::Name.eq(&dto.name))
            .filter(routes::Column::DeletedAt.is_null())
            .count(&self.db)
            .await
            .map_err(|_| ConrogateError::DatabaseInternal)?;
        if dup > 0 {
            return Err(ConrogateError::Conflict(format!(
                "route name '{}' already exists",
                dto.name
            )));
        }

        let active = convert::route_create_to_active_model(dto);
        let model = active
            .insert(&self.db)
            .await
            .map_err(|e| ConrogateError::DataMapping(e.to_string()))?;

        convert::route_model_to_dto(model).ok_or(ConrogateError::DataMapping(
            "insert returned no model".into(),
        ))
    }

    async fn update(&self, dto: UpdateRouteDto) -> Result<RouteDto, ConrogateError> {
        let model = RouteEntity::find_by_id(dto.id as i64)
            .filter(routes::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .map_err(|_| ConrogateError::DatabaseInternal)?
            .ok_or_else(|| ConrogateError::NotFound(format!("route {}", dto.id)))?;

        if let Some(ref name) = dto.name {
            if *name != model.name {
                let dup = RouteEntity::find()
                    .filter(routes::Column::Name.eq(name))
                    .filter(routes::Column::DeletedAt.is_null())
                    .filter(routes::Column::Id.ne(dto.id as i64))
                    .count(&self.db)
                    .await
                    .map_err(|_| ConrogateError::DatabaseInternal)?;
                if dup > 0 {
                    return Err(ConrogateError::Conflict(format!(
                        "route name '{name}' already exists"
                    )));
                }
            }
        }

        let mut active: routes::ActiveModel = model.into();
        if let Some(name) = dto.name {
            active.name = Set(name);
        }
        if let Some(mc) = dto.match_conditions {
            active.match_conditions = Set(serde_json::to_value(&mc).unwrap_or_default());
        }
        if let Some(priority) = dto.priority {
            active.priority = Set(priority);
        }
        if let Some(upstream_id) = dto.upstream_id {
            active.upstream_id = Set(Some(upstream_id as i64));
        }
        if let Some(host) = dto.host_header {
            active.host_header = Set(Some(host));
        }
        if let Some(retry) = dto.allow_retry_non_idempotent {
            active.allow_retry_non_idempotent = Set(retry);
        }
        if let Some(strip) = dto.ws_strip_sensitive_headers {
            active.ws_strip_sensitive_headers = Set(strip);
        }
        if let Some(enabled) = dto.enabled {
            active.enabled = Set(enabled);
        }
        active.updated_at = Set(chrono::Utc::now());

        let model = active
            .update(&self.db)
            .await
            .map_err(|e| ConrogateError::DataMapping(e.to_string()))?;

        convert::route_model_to_dto(model).ok_or(ConrogateError::DataMapping(
            "update returned no model".into(),
        ))
    }

    async fn soft_delete(&self, id: u64) -> Result<(), ConrogateError> {
        RouteEntity::update_many()
            .col_expr(
                routes::Column::DeletedAt,
                Expr::value(Some(chrono::Utc::now())),
            )
            .filter(routes::Column::Id.eq(id as i64))
            .filter(routes::Column::DeletedAt.is_null())
            .exec(&self.db)
            .await
            .map_err(|_| ConrogateError::DatabaseInternal)?;
        Ok(())
    }

    async fn list_paginated(
        &self,
        page: u32,
        page_size: u32,
    ) -> Result<PaginatedResult<RouteDto>, ConrogateError> {
        let page_size = page_size.clamp(1, 200);
        let query = RouteEntity::find()
            .filter(routes::Column::DeletedAt.is_null())
            .order_by_desc(routes::Column::Id);

        let total = query
            .clone()
            .count(&self.db)
            .await
            .map_err(|_| ConrogateError::DatabaseInternal)?;

        let models = query
            .offset(((page - 1) * page_size) as u64)
            .limit(page_size as u64)
            .all(&self.db)
            .await
            .map_err(|_| ConrogateError::DatabaseInternal)?;

        let list: Vec<RouteDto> = models
            .into_iter()
            .filter_map(convert::route_model_to_dto)
            .collect();
        Ok(PaginatedResult {
            list,
            total,
            page,
            page_size,
        })
    }
}
