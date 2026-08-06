<script setup lang="ts">
/**
 * 指标洞察页：全局概览卡片 + QPS/延迟/状态码/热门路由图表。
 * 对应控制面接口见 docs/api.md「7. 指标与洞察分析」。
 *
 * 图表基于 ECharts（按需注册：Canvas 渲染 + Line/Pie/Bar）。
 */
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { use } from 'echarts/core'
import { CanvasRenderer } from 'echarts/renderers'
import { BarChart, LineChart, PieChart } from 'echarts/charts'
import {
  GridComponent,
  LegendComponent,
  TooltipComponent,
} from 'echarts/components'
import VChart from 'vue-echarts'
import type { EChartsCoreOption } from 'echarts/core'
import { metricsApi } from '@/api/metrics'
import { useToastStore } from '@/stores/toast'
import AppCard from '@/components/ui/AppCard.vue'
import AppSelect from '@/components/ui/AppSelect.vue'
import type { LatencyResponse, QpsSeriesResponse, StatusCodesResponse, TopRoutesResponse } from '@/types/insights'
import type { OverviewMetric } from '@/types'

// 按需注册 ECharts 模块（减小打包体积）
use([CanvasRenderer, LineChart, PieChart, BarChart, GridComponent, TooltipComponent, LegendComponent])

// ── 状态 ──

const toast = useToastStore()

/** 时间范围（分钟）选项 */
const rangeOptions = [
  { value: 5, label: '最近 5 分钟' },
  { value: 10, label: '最近 10 分钟' },
  { value: 30, label: '最近 30 分钟' },
  { value: 60, label: '最近 1 小时' },
  { value: 180, label: '最近 3 小时' },
  { value: 1440, label: '最近 24 小时' },
]

const rangeMin = ref(5)
const autoRefresh = ref(true)
const loading = ref(false)

const overview = ref<OverviewMetric | null>(null)
const qps = ref<QpsSeriesResponse | null>(null)
const latency = ref<LatencyResponse | null>(null)
const statusCodes = ref<StatusCodesResponse | null>(null)
const topRoutes = ref<TopRoutesResponse | null>(null)

let timer: ReturnType<typeof setInterval> | null = null

// ── 数据加载 ──

/** 拉取全部指标接口（并行） */
async function load(): Promise<void> {
  loading.value = true
  try {
    const [o, q, l, s, t] = await Promise.all([
      metricsApi.overview(rangeMin.value),
      metricsApi.qpsSeries(rangeMin.value),
      metricsApi.latency(rangeMin.value),
      metricsApi.statusCodes(rangeMin.value),
      metricsApi.topRoutes(rangeMin.value),
    ])
    overview.value = o
    qps.value = q
    latency.value = l
    statusCodes.value = s
    topRoutes.value = t
  } catch (e) {
    toast.error((e as Error).message)
  } finally {
    loading.value = false
  }
}

/** 自动刷新开关 */
function toggleAutoRefresh(): void {
  if (autoRefresh.value) {
    startTimer()
  } else {
    stopTimer()
  }
}

function startTimer(): void {
  stopTimer()
  timer = setInterval(() => void load(), 10_000)
}

function stopTimer(): void {
  if (timer) {
    clearInterval(timer)
    timer = null
  }
}

// ── 图表 option 构造 ──

/** QPS 时序折线图 */
const qpsOption = computed<EChartsCoreOption>(() => ({
  tooltip: { trigger: 'axis' },
  grid: { left: 40, right: 16, top: 20, bottom: 24 },
  xAxis: { type: 'category', data: (qps.value?.series ?? []).map((p) => p.ts) },
  yAxis: { type: 'value', name: 'QPS' },
  series: [{ name: 'QPS', type: 'line', smooth: true, areaStyle: { opacity: 0.15 }, data: (qps.value?.series ?? []).map((p) => p.qps) }],
}))

/** 延迟百分位柱状图 */
const latencyOption = computed<EChartsCoreOption>(() => {
  const l = latency.value
  const items = [
    { name: '平均', value: l?.avg_ms ?? 0 },
    { name: 'P50', value: l?.p50_ms ?? 0 },
    { name: 'P95', value: l?.p95_ms ?? 0 },
    { name: 'P99', value: l?.p99_ms ?? 0 },
  ]
  return {
    tooltip: { trigger: 'axis' },
    grid: { left: 40, right: 16, top: 20, bottom: 24 },
    xAxis: { type: 'category', data: items.map((i) => i.name) },
    yAxis: { type: 'value', name: 'ms' },
    series: [{ name: '延迟', type: 'bar', barMaxWidth: 40, data: items.map((i) => i.value) }],
  }
})

/** 状态码分布环形图 */
const statusOption = computed<EChartsCoreOption>(() => {
  const s = statusCodes.value
  return {
    tooltip: { trigger: 'item', formatter: '{b}: {c} ({d}%)' },
    legend: { bottom: 0 },
    series: [
      {
        type: 'pie',
        radius: ['40%', '65%'],
        avoidLabelOverlap: true,
        itemStyle: { borderRadius: 4, borderColor: '#fff', borderWidth: 2 },
        data: [
          { name: '2xx', value: s?.['2xx'] ?? 0, itemStyle: { color: '#10b981' } },
          { name: '3xx', value: s?.['3xx'] ?? 0, itemStyle: { color: '#0ea5e9' } },
          { name: '4xx', value: s?.['4xx'] ?? 0, itemStyle: { color: '#f59e0b' } },
          { name: '5xx', value: s?.['5xx'] ?? 0, itemStyle: { color: '#ef4444' } },
        ],
      },
    ],
  }
})

/** 热门路由横向条形图（取前 10） */
const topRoutesOption = computed<EChartsCoreOption>(() => {
  const top = (topRoutes.value?.top_routes ?? []).slice(0, 10)
  return {
    tooltip: { trigger: 'axis' },
    grid: { left: 40, right: 30, top: 16, bottom: 24 },
    xAxis: { type: 'value', name: '请求数' },
    yAxis: { type: 'category', inverse: true, data: top.map((t) => (t.route_id ? `route#${t.route_id}` : 'all')) },
    series: [{ name: '请求数', type: 'bar', barMaxWidth: 16, data: top.map((t) => t.total_requests) }],
  }
})

// ── 格式化 ──

/** 错误率百分比展示 */
const errorRateText = computed(() => {
  const rate = overview.value?.error_rate ?? 0
  return `${(rate * 100).toFixed(2)}%`
})

// ── 生命周期 ──

onMounted(() => {
  void load()
  startTimer()
})

onBeforeUnmount(() => stopTimer())
</script>

<template>
  <div class="space-y-4">
    <!-- 顶部工具条 -->
    <div class="flex items-center justify-between">
      <AppSelect
        v-model="rangeMin"
        label=""
        :options="rangeOptions"
        class="w-40"
        @change="load"
      />
      <label class="flex items-center gap-1.5 text-sm text-slate-600">
        <input v-model="autoRefresh" type="checkbox" class="accent-indigo-600" @change="toggleAutoRefresh" />
        每 10s 自动刷新
      </label>
    </div>

    <!-- 概览卡片 -->
    <div class="grid grid-cols-2 gap-4 lg:grid-cols-4">
      <AppCard>
        <div class="text-xs text-slate-400">总 QPS</div>
        <div class="mt-1 text-2xl font-semibold text-slate-800">{{ (overview?.total_qps ?? 0).toFixed(1) }}</div>
      </AppCard>
      <AppCard>
        <div class="text-xs text-slate-400">平均延迟</div>
        <div class="mt-1 text-2xl font-semibold text-slate-800">{{ (overview?.avg_latency_ms ?? 0).toFixed(1) }} ms</div>
      </AppCard>
      <AppCard>
        <div class="text-xs text-slate-400">错误率</div>
        <div class="mt-1 text-2xl font-semibold text-slate-800">{{ errorRateText }}</div>
      </AppCard>
      <AppCard>
        <div class="text-xs text-slate-400">延迟 P99</div>
        <div class="mt-1 text-2xl font-semibold text-slate-800">{{ latency?.p99_ms ?? 0 }} ms</div>
      </AppCard>
    </div>

    <!-- 图表区（2x2） -->
    <div class="grid grid-cols-1 gap-4 lg:grid-cols-2">
      <AppCard title="QPS 时序">
        <VChart :option="qpsOption" autoresize style="height: 260px" />
      </AppCard>
      <AppCard title="延迟分布">
        <VChart :option="latencyOption" autoresize style="height: 260px" />
      </AppCard>
      <AppCard title="状态码分布">
        <VChart :option="statusOption" autoresize style="height: 260px" />
      </AppCard>
      <AppCard title="热门路由 TOP 10">
        <VChart :option="topRoutesOption" autoresize style="height: 260px" />
      </AppCard>
    </div>

    <p v-if="loading" class="text-center text-xs text-slate-400">刷新中...</p>
  </div>
</template>
