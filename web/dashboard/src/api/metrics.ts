/**
 * 指标与洞察 API。
 * 端点契约见 docs/api.md「7. 指标与洞察分析」。
 */
import { api } from '@/api/client'
import type { MetricRow, OverviewMetric } from '@/types'
import type {
  LatencyResponse,
  QpsSeriesResponse,
  StatusCodesResponse,
  TopRoutesResponse,
} from '@/types/insights'

export const metricsApi = {
  /** 原始指标行（时间桶维度） */
  rows: (rangeMin: number, filter?: { route_id?: number; gate_id?: string }) =>
    api.get<MetricRow[]>('/metrics', { range_min: rangeMin, ...filter }),

  /** 全局概览（总 QPS / 平均延迟 / 错误率） */
  overview: (rangeMin: number) =>
    api.get<OverviewMetric>('/metrics/overview', { range_min: rangeMin }),

  /** QPS 时序数据 */
  qpsSeries: (rangeMin: number) =>
    api.get<QpsSeriesResponse>('/insights/qps', { range_min: rangeMin }),

  /** 延迟百分位汇总 */
  latency: (rangeMin: number) =>
    api.get<LatencyResponse>('/insights/latency', { range_min: rangeMin }),

  /** 状态码分布 */
  statusCodes: (rangeMin: number) =>
    api.get<StatusCodesResponse>('/insights/status-codes', { range_min: rangeMin }),

  /** 热门路由排行 */
  topRoutes: (rangeMin: number) =>
    api.get<TopRoutesResponse>('/insights/top-routes', { range_min: rangeMin }),
}
