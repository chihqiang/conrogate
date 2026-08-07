//! 全局 IP 黑名单仓储实现。

use crate::contract::dto::*;
use crate::contract::storage::IpBlacklistRepo;
use crate::contract::ConrogateError;
use crate::storage::convert;
use crate::storage::entity::ip_blacklist::{self, Entity as BlacklistEntity};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder, QuerySelect, Set,
};

pub struct IpBlacklistRepoImpl {
    db: DatabaseConnection,
}

impl IpBlacklistRepoImpl {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl IpBlacklistRepo for IpBlacklistRepoImpl {
    async fn list_all(&self) -> Result<Vec<IpBlacklistDto>, ConrogateError> {
        let models = BlacklistEntity::find()
            .order_by_desc(ip_blacklist::Column::Id)
            .all(&self.db)
            .await
            .map_err(|_| ConrogateError::DatabaseInternal)?;
        Ok(models
            .into_iter()
            .filter_map(convert::ip_blacklist_model_to_dto)
            .collect())
    }

    async fn list_paginated(
        &self,
        filter: &IpBlacklistQuery,
        page: u32,
        page_size: u32,
    ) -> Result<PaginatedResult<IpBlacklistDto>, ConrogateError> {
        let page_size = page_size.clamp(1, 200);
        let mut query = BlacklistEntity::find();
        if let Some(kw) = filter.keyword.as_deref() {
            if !kw.trim().is_empty() {
                query = query.filter(ip_blacklist::Column::IpOrCidr.contains(kw.trim()));
            }
        }
        query = query.order_by_desc(ip_blacklist::Column::Id);

        let total = query
            .clone()
            .count(&self.db)
            .await
            .map_err(|_| ConrogateError::DatabaseInternal)?;
        let models = query
            .offset((page.saturating_sub(1) as u64) * page_size as u64)
            .limit(page_size as u64)
            .all(&self.db)
            .await
            .map_err(|_| ConrogateError::DatabaseInternal)?;

        let list = models
            .into_iter()
            .filter_map(convert::ip_blacklist_model_to_dto)
            .collect();
        Ok(PaginatedResult {
            list,
            total,
            page,
            page_size,
        })
    }

    async fn upsert(
        &self,
        dto: &CreateIpBlacklistDto,
        operator: Option<&str>,
    ) -> Result<IpBlacklistDto, ConrogateError> {
        let now = chrono::Utc::now();
        let expires_at = dto
            .expires_in_seconds
            .map(|secs| now + chrono::Duration::seconds(secs as i64));

        if let Some(existing) = BlacklistEntity::find()
            .filter(ip_blacklist::Column::IpOrCidr.eq(dto.ip_or_cidr.as_str()))
            .one(&self.db)
            .await
            .map_err(|_| ConrogateError::DatabaseInternal)?
        {
            let mut active: ip_blacklist::ActiveModel = existing.into();
            active.reason = Set(dto.reason.clone());
            active.expires_at = Set(expires_at);
            let model = active
                .update(&self.db)
                .await
                .map_err(|_| ConrogateError::DatabaseInternal)?;
            return convert::ip_blacklist_model_to_dto(model).ok_or(ConrogateError::DataMapping(
                "blacklist row unmappable".into(),
            ));
        }

        let active = ip_blacklist::ActiveModel {
            ip_or_cidr: Set(dto.ip_or_cidr.clone()),
            reason: Set(dto.reason.clone()),
            expires_at: Set(expires_at),
            created_by: Set(operator.map(ToString::to_string)),
            created_at: Set(now),
            ..Default::default()
        };
        let model = active
            .insert(&self.db)
            .await
            .map_err(|e| ConrogateError::DataMapping(e.to_string()))?;
        convert::ip_blacklist_model_to_dto(model)
            .ok_or_else(|| ConrogateError::DataMapping("blacklist row unmappable".into()))
    }

    async fn delete(&self, id: u64) -> Result<(), ConrogateError> {
        let result = BlacklistEntity::delete_by_id(id as i64)
            .exec(&self.db)
            .await
            .map_err(|_| ConrogateError::DatabaseInternal)?;
        if result.rows_affected == 0 {
            return Err(ConrogateError::NotFound("blacklist entry not found".into()));
        }
        Ok(())
    }
}
