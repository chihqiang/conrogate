/**
 * 安全 / IP 黑名单 API。
 * 端点契约见 docs/api.md「11.5 IP 黑名单管理」。
 */
import { api } from '@/api/client'
import type { PageQuery } from '@/api/client'
import type { CreateIpBlacklistPayload, IpBlacklistDto, PaginatedResult } from '@/types'

export interface IpBlacklistQuery extends PageQuery {
  keyword?: string
}

export const securityApi = {
  /** 分页查询黑名单（可按 IP/CIDR/备注模糊搜索） */
  list: (query: IpBlacklistQuery) => api.get<PaginatedResult<IpBlacklistDto>>('/security/ip_blacklist', query),
  /** 拉黑（幂等：重复拉黑同一 IP/CIDR 刷新 reason / expires_at） */
  create: (payload: CreateIpBlacklistPayload) => api.post<IpBlacklistDto>('/security/ip_blacklist', payload),
  /** 解除拉黑 */
  remove: (id: number) => api.delete<null>(`/security/ip_blacklist/${id}`),
}
