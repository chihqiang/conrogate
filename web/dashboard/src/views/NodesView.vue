<script setup lang="ts">
/**
 * 节点页：展示所有已注册网关节点及其配置版本应用状态。
 * 对应控制面接口见 docs/api.md「10. 节点管理」（GET /nodes 返回非分页数组）。
 */
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { nodeApi } from '@/api/nodes'
import { useToastStore } from '@/stores/toast'
import AppBadge from '@/components/ui/AppBadge.vue'
import AppButton from '@/components/ui/AppButton.vue'
import AppCard from '@/components/ui/AppCard.vue'
import AppEmpty from '@/components/ui/AppEmpty.vue'
import AppTable, { type TableColumn } from '@/components/ui/AppTable.vue'
import type { NodeApplicationRow } from '@/types'

// ── 状态 ──

const toast = useToastStore()

const nodes = ref<NodeApplicationRow[]>([])
const loading = ref(false)
const autoRefresh = ref(true)

let timer: ReturnType<typeof setInterval> | null = null

// ── 辅助函数 ──

function fmtTime(value: string): string {
  if (!value) return '-'
  const d = new Date(value)
  if (Number.isNaN(d.getTime())) return '-'
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`
}

/** 依据 last_seen 判断节点是否在线（60s 内心跳视为在线） */
function isOnline(lastSeen: string): boolean {
  if (!lastSeen) return false
  const t = Date.parse(lastSeen)
  if (Number.isNaN(t)) return false
  return Date.now() - t < 60_000
}

/** 在线节点数量 */
const onlineCount = computed(() => nodes.value.filter((n) => isOnline(n.last_seen)).length)

// ── 表格列 ──

const columns: TableColumn[] = [
  { key: 'status', label: '状态', width: '80px' },
  { key: 'gate_id', label: '节点 ID（gate_id）' },
  { key: 'version', label: '已应用版本', width: '110px' },
  { key: 'applied_at', label: '应用时间', width: '150px', formatter: (v) => fmtTime(String(v)) },
  { key: 'last_seen', label: '最近心跳', width: '150px', formatter: (v) => fmtTime(String(v)) },
]

// ── 数据加载 ──

async function load(): Promise<void> {
  loading.value = true
  try {
    nodes.value = await nodeApi.list()
  } catch (e) {
    toast.error((e as Error).message)
  } finally {
    loading.value = false
  }
}

function toggleAutoRefresh(): void {
  if (autoRefresh.value) {
    stopTimer()
    timer = setInterval(() => void load(), 15_000)
  } else {
    stopTimer()
  }
}

function stopTimer(): void {
  if (timer) {
    clearInterval(timer)
    timer = null
  }
}

// ── 生命周期 ──

onMounted(() => {
  void load()
  startTimer()
})

function startTimer(): void {
  stopTimer()
  timer = setInterval(() => void load(), 15_000)
}

onBeforeUnmount(() => stopTimer())
</script>

<template>
  <AppCard>
    <template #title>
      <div class="flex items-center gap-3">
        网关节点
        <span class="text-xs font-normal text-slate-400">在线 {{ onlineCount }} / 共 {{ nodes.length }}</span>
      </div>
    </template>
    <template #actions>
      <label class="flex items-center gap-1.5 text-sm text-slate-600">
        <input v-model="autoRefresh" type="checkbox" class="accent-indigo-600" @change="toggleAutoRefresh" />
        自动刷新
      </label>
      <AppButton size="sm" @click="load">刷新</AppButton>
    </template>

    <AppTable :columns="columns" :rows="nodes" :loading="loading">
      <template #cell-status="{ row }">
        <AppBadge :tone="isOnline((row as NodeApplicationRow).last_seen) ? 'green' : 'red'">
          {{ isOnline((row as NodeApplicationRow).last_seen) ? '在线' : '离线' }}
        </AppBadge>
      </template>
      <template #empty>
        <AppEmpty text="暂无已注册节点，数据面接入后会自动上报心跳" />
      </template>
    </AppTable>
  </AppCard>
</template>
