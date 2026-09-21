/**
 * 事件与节点 API。
 * 端点契约见 docs/api.md「8. 事件查询」与「10. 节点管理」。
 */
import { api } from '@/api/client'
import type { PageQuery } from '@/api/client'
import type { EventRow, NodeApplicationRow, PaginatedResult } from '@/types'

export interface EventQuery extends PageQuery {
  event_type?: string
  route_id?: number
  ts_from?: string
  ts_to?: string
}

export const eventApi = {
  /** 分页查询网关事件 */
  list: (query: EventQuery) => api.get<PaginatedResult<EventRow>>('/insights/events', query),
}

export const nodeApi = {
  /** 查询所有已注册网关节点 */
  list: () => api.get<NodeApplicationRow[]>('/nodes'),
}
