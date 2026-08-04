//! 事件仓储实现。

use crate::convert;
use crate::entity::gateway_events::{self, Entity as EventEntity};
use conrogate_contract::dto::{EventQuery, EventRow, PaginatedResult};
use conrogate_contract::storage::EventRepo;
use conrogate_contract::ConrogateError;
use sea_orm::sea_query::OnConflict;
use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect,
};

pub struct EventRepoImpl {
    db: DatabaseConnection,
}

impl EventRepoImpl {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl EventRepo for EventRepoImpl {
    async fn insert_batch(&self, events: &[EventRow]) -> Result<(), ConrogateError> {
        if events.is_empty() {
            return Ok(());
        }
        let actives: Vec<_> = events
            .iter()
            .map(convert::event_row_to_active_model)
            .collect();
        EventEntity::insert_many(actives)
            .on_conflict(
                OnConflict::columns([
                    gateway_events::Column::TraceId,
                    gateway_events::Column::Ts,
                    gateway_events::Column::EventType,
                ])
                .do_nothing_on([gateway_events::Column::Id])
                .to_owned(),
            )
            .exec(&self.db)
            .await
            .map_err(|e| ConrogateError::DataMapping(e.to_string()))?;
        Ok(())
    }

    async fn query(
        &self,
        filter: &EventQuery,
        page: u32,
        page_size: u32,
    ) -> Result<PaginatedResult<EventRow>, ConrogateError> {
        let page_size = page_size.clamp(1, 200);
        let mut query = EventEntity::find().order_by_desc(gateway_events::Column::Ts);

        if let Some(ref event_type) = filter.event_type {
            query = query.filter(gateway_events::Column::EventType.eq(event_type));
        }
        if let Some(route_id) = filter.route_id {
            query = query.filter(gateway_events::Column::RouteId.eq(route_id as i64));
        }
        if let Some(ts_from) = filter.ts_from {
            query = query.filter(gateway_events::Column::Ts.gte(ts_from));
        }
        if let Some(ts_to) = filter.ts_to {
            query = query.filter(gateway_events::Column::Ts.lte(ts_to));
        }

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

        let list: Vec<EventRow> = models
            .into_iter()
            .filter_map(convert::event_model_to_row)
            .collect();
        Ok(PaginatedResult {
            list,
            total,
            page,
            page_size,
        })
    }
}
