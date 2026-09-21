/**
 * 配置版本管理 API。
 * 端点契约见 docs/api.md「6. 配置版本管理」。
 */
import { api } from '@/api/client'
import type { PageQuery } from '@/api/client'
import type { ConfigDiff, ConfigVersionDto, PaginatedResult } from '@/types'

export const configApi = {
  /** 发布当前配置为新版本 */
  publish: (query: { base_version?: number; remark?: string }) =>
    api.post<ConfigVersionDto>('/configs/publish', undefined, query),

  /** 分页查询版本历史（最新在前） */
  versions: (query: PageQuery) =>
    api.get<PaginatedResult<ConfigVersionDto>>('/configs/versions', query),

  /** 回滚到指定版本（生成 publish_type=rollback 的新版本） */
  rollback: (version: number) =>
    api.post<ConfigVersionDto>(`/configs/versions/${version}/rollback`),

  /** 对比两个版本差异 */
  diff: (from: number, to: number) =>
    api.get<ConfigDiff>('/configs/diff', { from, to }),
}
