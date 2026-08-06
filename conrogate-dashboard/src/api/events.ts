/**
 * 审计日志 API。
 * 端点契约见 docs/api.md「9. 审计日志」。
 */
import { api } from '@/api/client'
import type { PageQuery } from '@/api/client'
import type { AuditLogRow, PaginatedResult } from '@/types'

export interface AuditLogQuery extends PageQuery {
  operator?: string
  action?: string
  resource?: string
  ts_from?: string
  ts_to?: string
}

export const auditApi = {
  /** 分页查询管理操作审计记录 */
  list: (query: AuditLogQuery) => api.get<PaginatedResult<AuditLogRow>>('/audit-logs', query),
}
