//! 插件绑定仓储实现。

use crate::convert;
use crate::entity::route_plugin_bindings::{self, Entity as BindingEntity};
use conrogate_contract::dto::{BindPluginDto, PluginBindingDto, UpdatePluginBindingDto};
use conrogate_contract::storage::{PluginBindingRepo, ReadOnlyPluginBindingRepo};
use conrogate_contract::ConrogateError;
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set, RelationTrait};
use sea_orm::sea_query::Expr;

pub struct PluginBindingRepoImpl {
    db: DatabaseConnection,
}

impl PluginBindingRepoImpl {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl ReadOnlyPluginBindingRepo for PluginBindingRepoImpl {
    async fn list_by_route(&self, route_id: u64) -> Result<Vec<PluginBindingDto>, ConrogateError> {
        let models = BindingEntity::find()
            .filter(route_plugin_bindings::Column::RouteId.eq(route_id as i64))
            .filter(route_plugin_bindings::Column::DeletedAt.is_null())
            .filter(route_plugin_bindings::Column::Enabled.eq(true))
            .order_by_asc(route_plugin_bindings::Column::Order)
            .all(&self.db)
            .await
            .map_err(|_| ConrogateError::DatabaseInternal)?;

        Ok(models.into_iter().filter_map(convert::binding_model_to_dto).collect())
    }
}

#[async_trait::async_trait]
impl PluginBindingRepo for PluginBindingRepoImpl {
    async fn bind(&self, route_id: u64, dto: BindPluginDto) -> Result<PluginBindingDto, ConrogateError> {
        let active = convert::binding_create_to_active_model(route_id as i64, dto);
        let model = active
            .insert(&self.db)
            .await
            .map_err(|e| ConrogateError::DataMapping(e.to_string()))?;

        convert::binding_model_to_dto(model)
            .ok_or(ConrogateError::DataMapping("insert returned no model".into()))
    }

    async fn update(
        &self,
        route_id: u64,
        plugin_name: &str,
        dto: UpdatePluginBindingDto,
    ) -> Result<PluginBindingDto, ConrogateError> {
        let model = BindingEntity::find()
            .filter(route_plugin_bindings::Column::RouteId.eq(route_id as i64))
            .filter(route_plugin_bindings::Column::PluginName.eq(plugin_name))
            .filter(route_plugin_bindings::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .map_err(|_| ConrogateError::DatabaseInternal)?
            .ok_or_else(|| ConrogateError::NotFound(format!("binding route={} plugin={}", route_id, plugin_name)))?;

        let mut active: route_plugin_bindings::ActiveModel = model.into();
        if let Some(config) = dto.config { active.config = Set(config); }
        if let Some(order) = dto.order { active.order = Set(order); }
        if let Some(blocking) = dto.blocking { active.blocking = Set(blocking); }
        if let Some(enabled) = dto.enabled { active.enabled = Set(enabled); }
        active.updated_at = Set(chrono::Utc::now());

        let model = active
            .update(&self.db)
            .await
            .map_err(|e| ConrogateError::DataMapping(e.to_string()))?;

        convert::binding_model_to_dto(model)
            .ok_or(ConrogateError::DataMapping("update returned no model".into()))
    }

    async fn unbind(&self, route_id: u64, plugin_name: &str) -> Result<(), ConrogateError> {
        BindingEntity::update_many()
            .col_expr(
                route_plugin_bindings::Column::DeletedAt,
                Expr::value(Some(chrono::Utc::now())),
            )
            .filter(route_plugin_bindings::Column::RouteId.eq(route_id as i64))
            .filter(route_plugin_bindings::Column::PluginName.eq(plugin_name))
            .filter(route_plugin_bindings::Column::DeletedAt.is_null())
            .exec(&self.db)
            .await
            .map_err(|_| ConrogateError::DatabaseInternal)?;
        Ok(())
    }
}
