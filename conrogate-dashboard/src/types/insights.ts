/**
 * 指标洞察接口的响应类型（独立文件，仅 MetricsView 使用）。
 * 响应结构来源：docs/api.md「7. 指标与洞察分析」。
 */

/** GET /insights/qps —— QPS 时序数据 */
export interface QpsSeriesPoint {
  ts: string
  qps: number
}

export interface QpsSeriesResponse {
  series: QpsSeriesPoint[]
}

/** GET /insights/latency —— 延迟百分位汇总 */
export interface LatencyResponse {
  avg_ms: number
  p50_ms: number
  p95_ms: number
  p99_ms: number
}

/** GET /insights/status-codes —— 状态码分布 */
export interface StatusCodesResponse {
  '2xx': number
  '3xx': number
  '4xx': number
  '5xx': number
}

/** GET /insights/top-routes —— 热门路由排行 */
export interface TopRouteEntry {
  route_id: number
  total_requests: number
}

export interface TopRoutesResponse {
  top_routes: TopRouteEntry[]
}
