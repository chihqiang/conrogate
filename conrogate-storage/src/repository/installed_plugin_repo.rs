//! 已安装插件仓储实现。

use crate::convert;
use crate::entity::installed_plugins::{self, Entity as PluginEntity};
use conrogate_contract::dto::InstalledPluginDto;
use conrogate_contract::plugin::PluginStatus;
use conrogate_contract::storage::InstalledPluginRepo;
use conrogate_contract::ConrogateError;
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder,
};

fn status_to_i16(s: PluginStatus) -> i16 {
    match s {
        PluginStatus::Installed => 0,
        PluginStatus::Active => 1,
        PluginStatus::Disabled => 2,
        PluginStatus::Uninstalled => 3,
    }
}

pub struct InstalledPluginRepoImpl {
    db: DatabaseConnection,
}

impl InstalledPluginRepoImpl {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl InstalledPluginRepo for InstalledPluginRepoImpl {
    async fn list(
        &self,
        status: Option<PluginStatus>,
    ) -> Result<Vec<InstalledPluginDto>, ConrogateError> {
        let mut query = PluginEntity::find()
            .filter(installed_plugins::Column::DeletedAt.is_null())
            .order_by_desc(installed_plugins::Column::InstalledAt);

        if let Some(s) = status {
            query = query.filter(installed_plugins::Column::Status.eq(status_to_i16(s)));
        }

        let models = query
            .all(&self.db)
            .await
            .map_err(|_| ConrogateError::DatabaseInternal)?;

        Ok(models
            .into_iter()
            .filter_map(convert::installed_plugin_model_to_dto)
            .collect())
    }

    async fn find_by_name(&self, name: &str) -> Result<Option<InstalledPluginDto>, ConrogateError> {
        let model = PluginEntity::find()
            .filter(installed_plugins::Column::Name.eq(name))
            .filter(installed_plugins::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .map_err(|_| ConrogateError::DatabaseInternal)?;

        Ok(model.and_then(convert::installed_plugin_model_to_dto))
    }

    async fn insert(&self, dto: &InstalledPluginDto) -> Result<(), ConrogateError> {
        let active = convert::installed_plugin_dto_to_active_model(dto);
        active
            .insert(&self.db)
            .await
            .map_err(|e| ConrogateError::DataMapping(e.to_string()))?;
        Ok(())
    }

    async fn update_status(&self, name: &str, status: PluginStatus) -> Result<(), ConrogateError> {
        PluginEntity::update_many()
            .col_expr(
                installed_plugins::Column::Status,
                Expr::value(status_to_i16(status)),
            )
            .filter(installed_plugins::Column::Name.eq(name))
            .filter(installed_plugins::Column::DeletedAt.is_null())
            .exec(&self.db)
            .await
            .map_err(|_| ConrogateError::DatabaseInternal)?;
        Ok(())
    }

    async fn soft_delete(&self, name: &str) -> Result<(), ConrogateError> {
        PluginEntity::update_many()
            .col_expr(
                installed_plugins::Column::DeletedAt,
                Expr::value(Some(chrono::Utc::now())),
            )
            .filter(installed_plugins::Column::Name.eq(name))
            .filter(installed_plugins::Column::DeletedAt.is_null())
            .exec(&self.db)
            .await
            .map_err(|_| ConrogateError::DatabaseInternal)?;
        Ok(())
    }
}
