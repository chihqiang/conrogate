<script setup lang="ts">
/**
 * 上游管理页：列表分页 + 新建/编辑（模态框）+ 删除。
 * 对应控制面接口见 docs/api.md「5. 上游管理」。
 */
import { onMounted, ref } from 'vue'
import { upstreamApi } from '@/api/upstreams'
import { useAuthStore } from '@/stores/auth'
import { useToastStore } from '@/stores/toast'
import AppBadge from '@/components/ui/AppBadge.vue'
import AppButton from '@/components/ui/AppButton.vue'
import AppCard from '@/components/ui/AppCard.vue'
import AppEmpty from '@/components/ui/AppEmpty.vue'
import AppPagination from '@/components/ui/AppPagination.vue'
import AppTable, { type TableColumn } from '@/components/ui/AppTable.vue'
import UpstreamDeleteModal from '@/components/features/UpstreamDeleteModal.vue'
import UpstreamFormModal from '@/components/features/UpstreamFormModal.vue'
import { BalancerAlgorithm, BalancerAlgorithmLabels } from '@/types/enums'
import type { UpstreamDto } from '@/types'

// ── 状态 ──

const auth = useAuthStore()
const toast = useToastStore()

const upstreams = ref<UpstreamDto[]>([])
const total = ref(0)
const page = ref(1)
const pageSize = 20
const loading = ref(false)

/** 新建/编辑弹窗：editing 为 null 表示新建 */
const formOpen = ref(false)
const editing = ref<UpstreamDto | null>(null)

/** 删除确认 */
const deleting = ref<UpstreamDto | null>(null)

// ── 辅助函数 ──

function fmtTime(value: string): string {
  if (!value) return '-'
  const d = new Date(value)
  if (Number.isNaN(d.getTime())) return '-'
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`
}

/** 节点列表 → 展示文本（地址，禁用节点标记 *） */
function nodesText(nodes: UpstreamDto['nodes']): string {
  if (nodes.length === 0) return '-'
  return nodes.map((n) => `${n.enabled ? '' : '*' }${n.address}`).join('，')
}

// ── 表格列定义 ──

const columns: TableColumn[] = [
  { key: 'id', label: 'ID', width: '56px' },
  { key: 'name', label: '名称' },
  { key: 'algorithm', label: '负载均衡', width: '130px' },
  { key: 'retry_enabled', label: '失败重试', width: '80px' },
  { key: 'nodes', label: '节点', formatter: (_, row) => nodesText((row as UpstreamDto).nodes) },
  { key: 'created_at', label: '创建时间', width: '150px', formatter: (v) => fmtTime(String(v)) },
  { key: 'actions', label: '操作', width: '150px', align: 'right' },
]

// ── 数据加载 ──

async function load(): Promise<void> {
  loading.value = true
  try {
    const res = await upstreamApi.list({ page: page.value, page_size: pageSize })
    upstreams.value = res.list
    total.value = res.total
  } catch (e) {
    toast.error((e as Error).message)
  } finally {
    loading.value = false
  }
}

// ── 新建 / 编辑 ──

function openCreate(): void {
  editing.value = null
  formOpen.value = true
}

function openEdit(row: UpstreamDto): void {
  editing.value = row
  formOpen.value = true
}

function onSaved(): void {
  void load()
}

// ── 删除 ──

function onDeleted(): void {
  // 当前页删空后回退一页
  if (upstreams.value.length === 1 && page.value > 1) page.value -= 1
  void load()
}

// ── 挂载 ──

onMounted(() => void load())
</script>

<template>
  <AppCard title="上游列表">
    <template #actions>
      <AppButton v-if="auth.canWrite" size="sm" @click="openCreate">新建上游</AppButton>
    </template>

    <AppTable :columns="columns" :rows="upstreams" :loading="loading">
      <!-- 负载均衡算法 -->
      <template #cell-algorithm="{ value }">
        <AppBadge tone="blue">{{ BalancerAlgorithmLabels[value as BalancerAlgorithm] }}</AppBadge>
      </template>

      <!-- 重试开关 -->
      <template #cell-retry_enabled="{ value }">
        <AppBadge :tone="value ? 'green' : 'gray'">{{ value ? '是' : '否' }}</AppBadge>
      </template>

      <!-- 操作列 -->
      <template #cell-actions="{ row }">
        <div class="flex items-center justify-end gap-1">
          <AppButton v-if="auth.canWrite" variant="secondary" size="sm" @click="openEdit(row as UpstreamDto)">
            编辑
          </AppButton>
          <AppButton v-if="auth.canWrite" variant="danger" size="sm" @click="deleting = row as UpstreamDto">
            删除
          </AppButton>
        </div>
      </template>

      <template #empty>
        <AppEmpty text="暂无上游，点击右上角「新建上游」创建" />
      </template>
    </AppTable>

    <AppPagination :total="total" :page="page" :page-size="pageSize" @update:page="page = $event; load()" />
  </AppCard>

  <!-- 新建 / 编辑弹窗 -->
  <UpstreamFormModal v-model:open="formOpen" :upstream="editing" @saved="onSaved" />

  <!-- 删除确认 -->
  <UpstreamDeleteModal :upstream="deleting" @close="deleting = null" @deleted="onDeleted" />
</template>
