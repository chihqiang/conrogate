//! 审计日志记录。

use crate::contract::dto::AuditLogRow;
use crate::contract::storage::AuditLogRepo;
use std::sync::Arc;

/// 审计服务
pub struct AuditService {
    repo: Arc<dyn AuditLogRepo>,
}

impl AuditService {
    pub fn new(repo: Arc<dyn AuditLogRepo>) -> Self {
        Self { repo }
    }

    /// 记录审计日志
    pub async fn log(
        &self,
        operator: Option<&str>,
        action: &str,
        resource: &str,
        resource_id: Option<u64>,
        detail: serde_json::Value,
        trace_id: Option<String>,
    ) {
        let row = AuditLogRow {
            ts: chrono::Utc::now(),
            operator: operator.map(|s| s.to_string()),
            action: action.to_string(),
            resource: resource.to_string(),
            resource_id,
            detail,
            trace_id,
        };

        if let Err(e) = self.repo.insert(&row).await {
            tracing::warn!(error = %e, action = %action, "audit log insert failed");
        }
    }
}
