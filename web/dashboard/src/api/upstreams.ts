/**
 * 上游管理 API。
 * 端点契约见 docs/api.md「5. 上游管理」。
 */
import { api } from '@/api/client'
import type { PageQuery } from '@/api/client'
import type {
  CreateUpstreamPayload,
  PaginatedResult,
  UpdateUpstreamPayload,
  UpstreamDto,
  UpstreamRouteBindingDto,
} from '@/types'

export const upstreamApi = {
  /** 分页查询上游列表 */
  list: (query: PageQuery) => api.get<PaginatedResult<UpstreamDto>>('/upstreams', query),

  /** 查询上游详情 */
  get: (id: number) => api.get<UpstreamDto>(`/upstreams/${id}`),

  /** 创建上游（含后端节点） */
  create: (payload: CreateUpstreamPayload) => api.post<UpstreamDto>('/upstreams', payload),

  /** 更新上游 */
  update: (payload: UpdateUpstreamPayload) => api.put<UpstreamDto>(`/upstreams/${payload.id}`, payload),

  /** 删除上游（软删除；被路由绑定将被拒绝） */
  remove: (id: number) => api.delete<null>(`/upstreams/${id}`),

  /** 查询引用该上游的活跃路由（删除弹窗展示绑定关系） */
  routeBindings: (id: number) => api.get<UpstreamRouteBindingDto[]>(`/upstreams/${id}/route-bindings`),
}
