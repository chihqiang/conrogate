/**
 * 控制面 API 请求客户端。
 *
 * - 统一拼接 `VITE_API_BASE`（默认 `/api`，含 `/api/v1` 前缀）
 * - 自动携带 `Authorization: Bearer <operator:secret:role>` 鉴权头
 * - 解包后端统一信封 `ApiEnvelope`，`code !== 0` 抛 `ApiError`
 * - HTTP 401（未认证/token 失效）自动清除 token 并跳转登录页
 */
import { router } from '@/router'
import type { ApiEnvelope } from '@/types'

/** localStorage 中鉴权 token 的存储键（auth store 与 client 共用） */
export const TOKEN_KEY = 'conrogate.dashboard.token'

/** 控制面 API 基础路径（控制面默认挂载于 /api/v1 前缀，见 docs/api.md） */
const BASE = import.meta.env.VITE_API_BASE ?? '/api/v1'

/** 鉴权 token 读取 */
export function getToken(): string {
  return localStorage.getItem(TOKEN_KEY) ?? ''
}

/** 鉴权 token 写入 */
export function setToken(token: string): void {
  localStorage.setItem(TOKEN_KEY, token)
}

/** 清除本地鉴权状态 */
export function clearToken(): void {
  localStorage.removeItem(TOKEN_KEY)
}

/** 业务错误：携带后端错误码与请求追踪 ID */
export class ApiError extends Error {
  /** 后端错误码（0 为成功，见 docs/api.md 错误码表） */
  readonly code: number
  /** HTTP 状态码 */
  readonly status: number
  /** 后端返回的请求追踪 ID */
  readonly traceId?: string

  constructor(code: number, message: string, status: number, traceId?: string) {
    super(message)
    this.name = 'ApiError'
    this.code = code
    this.status = status
    this.traceId = traceId
  }
}

/** 请求配置 */
interface RequestConfig {
  method?: 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE'
  /** 查询参数；值为 undefined/null/空串 时自动省略 */
  query?: object
  /** JSON 请求体 */
  body?: unknown
}

/** 发起请求并返回信封内的 `data` */
async function request<T>(path: string, config: RequestConfig = {}): Promise<T> {
  const method = config.method ?? 'GET'

  // 组装查询串
  const search = new URLSearchParams()
  for (const [key, value] of Object.entries(config.query ?? {})) {
    if (value !== undefined && value !== null && value !== '') {
      search.set(key, String(value))
    }
  }
  const query = search.toString()
  const url = `${BASE}${path}${query ? `?${query}` : ''}`

  // 请求头：默认 JSON + 鉴权 token
  const headers: Record<string, string> = { 'Content-Type': 'application/json' }
  const token = getToken()
  if (token) {
    headers.Authorization = `Bearer ${token}`
  }

  let res: Response
  try {
    res = await fetch(url, {
      method,
      headers,
      body: config.body !== undefined ? JSON.stringify(config.body) : undefined,
    })
  } catch {
    // 网络层错误（连接被拒/超时/跨域被拦截等）
    throw new ApiError(-1, `无法连接控制面：${url}`, 0)
  }

  // 解包统一信封
  let envelope: ApiEnvelope<unknown>
  try {
    envelope = (await res.json()) as ApiEnvelope<unknown>
  } catch {
    throw new ApiError(res.status, `控制面返回非 JSON 响应（HTTP ${res.status}）`, res.status)
  }

  // 未认证：清除本地 token 并回登录页
  if (res.status === 401 || envelope.code === 10002) {
    clearToken()
    if (router.currentRoute.value.name !== 'login') {
      void router.replace({ name: 'login' })
    }
    throw new ApiError(envelope.code, envelope.msg || '未认证', res.status, envelope.trace_id)
  }

  if (envelope.code !== 0) {
    throw new ApiError(envelope.code, envelope.msg || '请求失败', res.status, envelope.trace_id)
  }

  return envelope.data as T
}

/** 导出统一的 HTTP 动词封装 */
export const api = {
  get: <T>(path: string, query?: RequestConfig['query']) => request<T>(path, { method: 'GET', query }),
  post: <T>(path: string, body?: unknown, query?: RequestConfig['query']) =>
    request<T>(path, { method: 'POST', body, query }),
  put: <T>(path: string, body?: unknown) => request<T>(path, { method: 'PUT', body }),
  patch: <T>(path: string, body?: unknown) => request<T>(path, { method: 'PATCH', body }),
  delete: <T>(path: string) => request<T>(path, { method: 'DELETE' }),
}

/** 分页查询参数 */
export interface PageQuery {
  page: number
  page_size: number
}
