/**
 * DTO 类型定义，与后端控制面 REST API 契约一一对应。
 *
 * 字段命名沿用后端 snake_case，避免传输层转换。
 * 后端契约来源：conrogate-core/src/contract/dto.rs、protocol.rs、balancer.rs。
 */
import type {
  BalancerAlgorithm,
  HttpMethod,
  MatchOp,
  PublishType,
  RouteProtocol,
} from '@/types/enums'

/** 控制面统一响应信封 */
export interface ApiEnvelope<T> {
  /** 0 表示成功；非零为错误码（见 docs/api.md 错误码表） */
  code: number
  /** 成功时为 "success"，失败时为错误描述 */
  msg: string
  data: T | null
  /** 32 位十六进制请求追踪 ID */
  trace_id: string
}

/** 通用分页结果 */
export interface PaginatedResult<T> {
  list: T[]
  total: number
  page: number
  page_size: number
}

// ── 路由 ──

/** 路径匹配条件（后端 PathMatch 为 tagged union：同一时刻只出现一个键） */
export type PathMatchUnion =
  | { prefix: string }
  | { exact: string }
  | { regex: string }

/** Header 匹配条件 */
export interface HeaderMatch {
  key: string
  op: MatchOp
  value: string
}

/** Query 参数匹配条件 */
export interface QueryMatch {
  key: string
  op: MatchOp
  value: string
}

/** 路由匹配条件集合（多维匹配，全部条件为 AND 关系） */
export interface RouteMatchConditions {
  path: PathMatchUnion
  methods: HttpMethod[] | null
  host: string | null
  headers: HeaderMatch[]
  query_params: QueryMatch[]
}

/** 路由（查询返回） */
export interface RouteDto {
  id: number
  name: string
  protocol: RouteProtocol
  match_conditions: RouteMatchConditions
  priority: number
  upstream_id: number | null
  host_header: string | null
  allow_retry_non_idempotent: boolean
  /** WS 隧道转发上游时是否剥离敏感头 */
  ws_strip_sensitive_headers: boolean
  enabled: boolean
  created_at: string
  updated_at: string
}

/** 创建路由请求体（后端 CreateRouteDto，所有可选字段均可省略） */
export interface CreateRoutePayload {
  name: string
  protocol: RouteProtocol
  match_conditions: RouteMatchConditions
  priority?: number | null
  upstream_id?: number | null
  host_header?: string | null
  allow_retry_non_idempotent?: boolean
  ws_strip_sensitive_headers?: boolean
  enabled?: boolean
}

/** 更新路由请求体（后端 UpdateRouteDto） */
export interface UpdateRoutePayload {
  id: number
  name?: string | null
  match_conditions?: RouteMatchConditions
  priority?: number | null
  upstream_id?: number | null
  host_header?: string | null
  allow_retry_non_idempotent?: boolean
  ws_strip_sensitive_headers?: boolean
  enabled?: boolean
}

// ── 上游 ──

/** 上游节点（查询返回） */
export interface UpstreamNodeDto {
  id: number
  upstream_id: number
  address: string
  weight: number
  enabled: boolean
}

/** 上游（查询返回） */
export interface UpstreamDto {
  id: number
  name: string
  algorithm: BalancerAlgorithm
  retry_enabled: boolean
  nodes: UpstreamNodeDto[]
  created_at: string
  updated_at: string
}

/** 创建上游的节点输入 */
export interface CreateUpstreamNodePayload {
  address: string
  weight?: number | null
  enabled?: boolean | null
}

/** 创建上游请求体 */
export interface CreateUpstreamPayload {
  name: string
  algorithm: BalancerAlgorithm
  retry_enabled?: boolean | null
  nodes: CreateUpstreamNodePayload[]
}

/** 更新上游请求体 */
export interface UpdateUpstreamPayload {
  id: number
  name?: string | null
  algorithm?: BalancerAlgorithm | null
  retry_enabled?: boolean | null
  nodes?: CreateUpstreamNodePayload[] | null
}

// ── 插件绑定 ──

/** 插件绑定记录（查询返回） */
export interface PluginBindingDto {
  id: number
  route_id: number
  plugin_name: string
  config: Record<string, unknown> | null
  order: number
  blocking: boolean
  enabled: boolean
}

/** 绑定插件请求体 */
export interface BindPluginPayload {
  plugin_name: string
  config: Record<string, unknown> | null
  order?: number | null
  blocking?: boolean | null
  enabled?: boolean | null
}

/** 更新插件绑定请求体 */
export interface UpdatePluginBindingPayload {
  config?: Record<string, unknown> | null
  order?: number | null
  blocking?: boolean | null
  enabled?: boolean | null
}

/** 已安装插件信息 */
export interface InstalledPluginDto {
  name: string
  version: string
  api_version: number
  kind: 'native' | 'wasm'
  status: 'installed' | 'active' | 'disabled' | 'uninstalled'
  package_hash: string | null
  manifest: Record<string, unknown>
  installed_at: string
  activated_at: string | null
}

// ── 配置版本 ──

/** 配置版本（发布 / 回滚记录） */
export interface ConfigVersionDto {
  version: number
  base_version: number
  publish_type: PublishType
  content_hash: string
  created_by: string | null
  remark: string | null
  applied_count: number
  created_at: string
}

/** 配置快照 */
export interface ConfigSnapshot {
  routes: RouteDto[]
  upstreams: UpstreamDto[]
  plugin_bindings: PluginBindingDto[]
}

/** 版本差异（人类可读的资源变更描述） */
export interface ConfigDiff {
  added: string[]
  modified: string[]
  removed: string[]
}

// ── 指标与事件 ──

/** 指标行（时间桶维度） */
export interface MetricRow {
  ts: string
  bucket_sec: number
  route_id: number | null
  gate_id: string
  qps: number
  total_requests: number
  avg_latency_ms: number
  p50_ms: number
  p90_ms: number
  p99_ms: number
  status_2xx: number
  status_3xx: number
  status_4xx: number
  status_5xx: number
  sessions: number
  bytes_in: number
  bytes_out: number
}

/** 指标概览 */
export interface OverviewMetric {
  total_qps: number
  avg_latency_ms: number
  error_rate: number
}

/** 网关事件行 */
export interface EventRow {
  ts: string
  event_type: string
  route_id: number | null
  upstream_id: number | null
  trace_id: string | null
  detail: Record<string, unknown> | null
}

/** 审计日志行 */
export interface AuditLogRow {
  ts: string
  operator: string | null
  action: string
  resource: string
  resource_id: number | null
  detail: Record<string, unknown> | null
  trace_id: string | null
}

/** 网关节点应用记录（分离模式心跳填充） */
export interface NodeApplicationRow {
  gate_id: string
  version: number
  applied_at: string
  last_seen: string
  updated_at: string
}

// ── 全局 IP 黑名单 ──

/** 黑名单条目（查询返回） */
export interface IpBlacklistDto {
  id: number
  /** IP 或 CIDR 网段（IPv4 / IPv6） */
  ip_or_cidr: string
  reason: string | null
  /** 过期时间；null = 永久 */
  expires_at: string | null
  created_by: string | null
  created_at: string
}

/** 拉黑请求体 */
export interface CreateIpBlacklistPayload {
  ip_or_cidr: string
  reason?: string | null
  /** 拉黑时长（秒）；缺省 = 永久 */
  expires_in_seconds?: number | null
}
