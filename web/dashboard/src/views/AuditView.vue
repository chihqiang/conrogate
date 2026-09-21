<script setup lang="ts">
/**
 * 审计日志页：分页展示管理操作审计记录。
 * 对应控制面接口见 docs/api.md「9. 审计日志」。
 */
import { onMounted, ref } from 'vue'
import { auditApi } from '@/api/events'
import { useToastStore } from '@/stores/toast'
import AppButton from '@/components/ui/AppButton.vue'
import AppCard from '@/components/ui/AppCard.vue'
import AppEmpty from '@/components/ui/AppEmpty.vue'
import AppInput from '@/components/ui/AppInput.vue'
import AppPagination from '@/components/ui/AppPagination.vue'
import AppSelect from '@/components/ui/AppSelect.vue'
import AppTable, { type TableColumn } from '@/components/ui/AppTable.vue'
import type { AuditLogRow } from '@/types'

// ── 状态 ──

const toast = useToastStore()

const logs = ref<AuditLogRow[]>([])
const total = ref(0)
const page = ref(1)
const pageSize = 20
const loading = ref(false)

// 筛选条件
const operator = ref('')
const action = ref('')
const resource = ref('')

/** 常见操作类型（用于下拉筛选） */
const actionOptions = [
  { value: '', label: '全部动作' },
  { value: 'create', label: '创建 (create)' },
  { value: 'update', label: '更新 (update)' },
  { value: 'delete', label: '删除 (delete)' },
  { value: 'publish', label: '发布 (publish)' },
  { value: 'rollback', label: '回滚 (rollback)' },
  { value: 'login', label: '登录 (login)' },
  { value: 'logout', label: '登出 (logout)' },
]

// ── 辅助函数 ──

function fmtTime(value: string): string {
  if (!value) return '-'
  const d = new Date(value)
  if (Number.isNaN(d.getTime())) return '-'
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`
}

function fmtDetail(detail: Record<string, unknown> | null): string {
  if (!detail) return '-'
  try {
    return JSON.stringify(detail)
  } catch {
    return '-'
  }
}

// ── 表格列 ──

const columns: TableColumn[] = [
  { key: 'ts', label: '时间', width: '150px', formatter: (v) => fmtTime(String(v)) },
  { key: 'operator', label: '操作人', width: '120px', formatter: (v) => String(v ?? '-') },
  { key: 'action', label: '动作', width: '90px' },
  { key: 'resource', label: '资源', width: '110px' },
  { key: 'resource_id', label: '资源 ID', width: '90px', formatter: (v) => (v === null || v === undefined ? '-' : String(v)) },
  { key: 'detail', label: '详情' },
  { key: 'trace_id', label: 'Trace ID', width: '160px', formatter: (v) => String(v ?? '-') },
]

// ── 数据加载 ──

async function load(): Promise<void> {
  loading.value = true
  try {
    const res = await auditApi.list({
      page: page.value,
      page_size: pageSize,
      operator: operator.value.trim() || undefined,
      action: action.value || undefined,
      resource: resource.value.trim() || undefined,
    })
    logs.value = res.list
    total.value = res.total
  } catch (e) {
    toast.error((e as Error).message)
  } finally {
    loading.value = false
  }
}

function search(): void {
  page.value = 1
  void load()
}

function reset(): void {
  operator.value = ''
  action.value = ''
  resource.value = ''
  page.value = 1
  void load()
}

// ── 挂载 ──

onMounted(() => void load())
</script>

<template>
  <AppCard title="审计日志">
    <!-- 筛选区 -->
    <template #header>
      <div class="mb-4 grid grid-cols-1 gap-3 md:grid-cols-4">
        <AppInput v-model="operator" label="操作人" placeholder="按操作人过滤" @keyup.enter="search" />
        <AppSelect v-model="action" label="动作" :options="actionOptions" @change="search" />
        <AppInput v-model="resource" label="资源" placeholder="如 route / upstream" @keyup.enter="search" />
        <div class="flex items-end gap-2">
          <AppButton size="sm" @click="search">查询</AppButton>
          <AppButton size="sm" variant="secondary" @click="reset">重置</AppButton>
        </div>
      </div>
    </template>

    <AppTable :columns="columns" :rows="logs" :loading="loading">
      <template #cell-detail="{ value }">
        <code class="block max-w-md truncate text-xs text-slate-500">{{ fmtDetail(value as Record<string, unknown> | null) }}</code>
      </template>
      <template #empty>
        <AppEmpty text="暂无审计记录" />
      </template>
    </AppTable>

    <AppPagination :total="total" :page="page" :page-size="pageSize" @update:page="page = $event; load()" />
  </AppCard>
</template>
