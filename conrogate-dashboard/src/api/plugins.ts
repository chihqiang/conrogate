/**
 * 插件管理 + 路由插件绑定 API。
 * 端点契约见 docs/api.md（插件管理）与控制面 openapi（/routes/:id/plugins）。
 */
import { api } from '@/api/client'
import type { BindPluginPayload, InstalledPluginDto, PluginBindingDto, UpdatePluginBindingPayload } from '@/types'

export const pluginApi = {
  /** 查询已安装插件（可按状态过滤） */
  list: (status?: string) => api.get<InstalledPluginDto[]>('/plugins', status ? { status } : undefined),

  /** 激活插件（Admin） */
  activate: (name: string) => api.post<null>(`/plugins/${name}/activate`),

  /** 停用插件（Admin） */
  disable: (name: string) => api.post<null>(`/plugins/${name}/disable`),

  /** 卸载插件（Admin） */
  uninstall: (name: string) => api.delete<null>(`/plugins/${name}`),

  /** 查询路由已绑定的插件 */
  bindings: (routeId: number) => api.get<PluginBindingDto[]>(`/routes/${routeId}/plugins`),

  /** 绑定插件到路由 */
  bind: (routeId: number, payload: BindPluginPayload) => api.post<PluginBindingDto>(`/routes/${routeId}/plugins`, payload),

  /** 更新路由上的插件绑定（配置/顺序/开关） */
  updateBinding: (routeId: number, pluginName: string, payload: UpdatePluginBindingPayload) =>
    api.put<PluginBindingDto>(`/routes/${routeId}/plugins/${pluginName}`, payload),

  /** 解绑路由上的插件 */
  unbind: (routeId: number, pluginName: string) => api.delete<null>(`/routes/${routeId}/plugins/${pluginName}`),
}
