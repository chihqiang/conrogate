//! 上游仓储实现。

use crate::contract::dto::*;
use crate::contract::storage::{ReadOnlyUpstreamRepo, UpstreamRepo};
use crate::contract::ConrogateError;
use crate::storage::convert;
use crate::storage::entity::{
    upstream_nodes,
    upstreams::{self, Entity as UpstreamEntity},
};
use sea_orm::{
    sea_query::Expr, ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set,
};

pub struct UpstreamRepoImpl {
    db: DatabaseConnection,
}

impl UpstreamRepoImpl {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    async fn load_nodes(
        &self,
        upstream_id: i64,
    ) -> Result<Vec<upstream_nodes::Model>, ConrogateError> {
        upstream_nodes::Entity::find()
            .filter(upstream_nodes::Column::UpstreamId.eq(upstream_id))
            .filter(upstream_nodes::Column::DeletedAt.is_null())
            .all(&self.db)
            .await
            .map_err(|_| ConrogateError::DatabaseInternal)
    }
}

#[async_trait::async_trait]
impl ReadOnlyUpstreamRepo for UpstreamRepoImpl {
    async fn list_all(&self) -> Result<Vec<UpstreamDto>, ConrogateError> {
        let models = UpstreamEntity::find()
            .filter(upstreams::Column::DeletedAt.is_null())
            .all(&self.db)
            .await
            .map_err(|_| ConrogateError::DatabaseInternal)?;

        let mut result = Vec::with_capacity(models.len());
        for m in models {
            let nodes = self.load_nodes(m.id).await?;
            result.push(convert::upstream_model_to_dto(m, nodes).ok_or(
                ConrogateError::DataMapping("upstream convert failed".into()),
            )?);
        }
        Ok(result)
    }

    async fn find_by_id(&self, id: u64) -> Result<Option<UpstreamDto>, ConrogateError> {
        let model = UpstreamEntity::find_by_id(id as i64)
            .filter(upstreams::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .map_err(|_| ConrogateError::DatabaseInternal)?;

        match model {
            Some(m) => {
                let nodes = self.load_nodes(m.id).await?;
                Ok(convert::upstream_model_to_dto(m, nodes))
            }
            None => Ok(None),
        }
    }

    async fn find_by_route(&self, route_id: u64) -> Result<Option<UpstreamDto>, ConrogateError> {
        // 通过 route → upstream_id 查找
        let route = crate::storage::entity::routes::Entity::find_by_id(route_id as i64)
            .filter(crate::storage::entity::routes::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .map_err(|_| ConrogateError::DatabaseInternal)?;

        match route.and_then(|r| r.upstream_id) {
            Some(upstream_id) => self.find_by_id(upstream_id as u64).await,
            None => Ok(None),
        }
    }
}

#[async_trait::async_trait]
impl UpstreamRepo for UpstreamRepoImpl {
    async fn create(&self, dto: CreateUpstreamDto) -> Result<UpstreamDto, ConrogateError> {
        let active = convert::upstream_create_to_active_model(dto.clone());
        let model = active
            .insert(&self.db)
            .await
            .map_err(|e| ConrogateError::DataMapping(e.to_string()))?;

        // 插入节点
        for node_dto in &dto.nodes {
            let node_active = convert::node_create_to_active_model(model.id, node_dto.clone());
            node_active
                .insert(&self.db)
                .await
                .map_err(|e| ConrogateError::DataMapping(e.to_string()))?;
        }

        let nodes = self.load_nodes(model.id).await?;
        convert::upstream_model_to_dto(model, nodes).ok_or(ConrogateError::DataMapping(
            "insert returned no model".into(),
        ))
    }

    async fn update(&self, dto: UpdateUpstreamDto) -> Result<UpstreamDto, ConrogateError> {
        let model = UpstreamEntity::find_by_id(dto.id as i64)
            .filter(upstreams::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .map_err(|_| ConrogateError::DatabaseInternal)?
            .ok_or_else(|| ConrogateError::NotFound(format!("upstream {}", dto.id)))?;

        let mut active: upstreams::ActiveModel = model.into();
        if let Some(name) = dto.name {
            active.name = Set(name);
        }
        if let Some(algo) = dto.algorithm {
            active.algorithm = Set(crate::storage::convert::algorithm_to_i16(algo));
        }
        if let Some(retry) = dto.retry_enabled {
            active.retry_enabled = Set(retry);
        }
        active.updated_at = Set(chrono::Utc::now());

        let model = active
            .update(&self.db)
            .await
            .map_err(|e| ConrogateError::DataMapping(e.to_string()))?;

        // 如果提供了新节点列表，替换旧节点
        if let Some(nodes) = dto.nodes {
            // 软删旧节点
            upstream_nodes::Entity::update_many()
                .col_expr(
                    upstream_nodes::Column::DeletedAt,
                    Expr::value(Some(chrono::Utc::now())),
                )
                .filter(upstream_nodes::Column::UpstreamId.eq(model.id))
                .filter(upstream_nodes::Column::DeletedAt.is_null())
                .exec(&self.db)
                .await
                .map_err(|_| ConrogateError::DatabaseInternal)?;

            // 插入新节点
            for node_dto in nodes {
                let node_active = convert::node_create_to_active_model(model.id, node_dto);
                node_active
                    .insert(&self.db)
                    .await
                    .map_err(|e| ConrogateError::DataMapping(e.to_string()))?;
            }
        }

        let nodes = self.load_nodes(model.id).await?;
        convert::upstream_model_to_dto(model, nodes).ok_or(ConrogateError::DataMapping(
            "update returned no model".into(),
        ))
    }

    async fn soft_delete(&self, id: u64) -> Result<(), ConrogateError> {
        // 软删上游
        UpstreamEntity::update_many()
            .col_expr(
                upstreams::Column::DeletedAt,
                Expr::value(Some(chrono::Utc::now())),
            )
            .filter(upstreams::Column::Id.eq(id as i64))
            .filter(upstreams::Column::DeletedAt.is_null())
            .exec(&self.db)
            .await
            .map_err(|_| ConrogateError::DatabaseInternal)?;

        // 软删关联节点
        upstream_nodes::Entity::update_many()
            .col_expr(
                upstream_nodes::Column::DeletedAt,
                Expr::value(Some(chrono::Utc::now())),
            )
            .filter(upstream_nodes::Column::UpstreamId.eq(id as i64))
            .filter(upstream_nodes::Column::DeletedAt.is_null())
            .exec(&self.db)
            .await
            .map_err(|_| ConrogateError::DatabaseInternal)?;

        Ok(())
    }

    async fn list_paginated(
        &self,
        page: u32,
        page_size: u32,
    ) -> Result<PaginatedResult<UpstreamDto>, ConrogateError> {
        let page_size = page_size.clamp(1, 200);
        let query = UpstreamEntity::find()
            .filter(upstreams::Column::DeletedAt.is_null())
            .order_by_desc(upstreams::Column::Id);

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

        let mut list = Vec::with_capacity(models.len());
        for m in models {
            let nodes = self.load_nodes(m.id).await?;
            if let Some(dto) = convert::upstream_model_to_dto(m, nodes) {
                list.push(dto);
            }
        }

        Ok(PaginatedResult {
            list,
            total,
            page,
            page_size,
        })
    }

    async fn list_route_bindings(
        &self,
        id: u64,
    ) -> Result<Vec<UpstreamRouteBindingDto>, ConrogateError> {
        let models = crate::storage::entity::routes::Entity::find()
            .filter(crate::storage::entity::routes::Column::UpstreamId.eq(id as i64))
            .filter(crate::storage::entity::routes::Column::DeletedAt.is_null())
            .all(&self.db)
            .await
            .map_err(|_| ConrogateError::DatabaseInternal)?;

        Ok(models
            .into_iter()
            .map(|m| UpstreamRouteBindingDto {
                id: m.id as u64,
                name: m.name,
            })
            .collect())
    }
}
