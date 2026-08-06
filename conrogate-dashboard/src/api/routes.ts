/**
 * 路由管理 API。
 * 端点契约见 docs/api.md「4. 路由管理」。
 */
import { api } from '@/api/client'
import type { PageQuery } from '@/api/client'
import type {
  CreateRoutePayload,
  PaginatedResult,
  RouteDto,
  UpdateRoutePayload,
} from '@/types'

export const routeApi = {
  /** 分页查询路由列表 */
  list: (query: PageQuery) => api.get<PaginatedResult<RouteDto>>('/routes', query),

  /** 查询路由详情 */
  get: (id: number) => api.get<RouteDto>(`/routes/${id}`),

  /** 创建路由 */
  create: (payload: CreateRoutePayload) => api.post<RouteDto>('/routes', payload),

  /** 整体更新路由 */
  update: (payload: UpdateRoutePayload) => api.put<RouteDto>(`/routes/${payload.id}`, payload),

  /** 局部更新路由 */
  patch: (id: number, payload: Partial<UpdateRoutePayload>) =>
    api.patch<RouteDto>(`/routes/${id}`, payload),

  /** 删除路由（软删除） */
  remove: (id: number) => api.delete<null>(`/routes/${id}`),
}
