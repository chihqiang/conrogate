<script setup lang="ts">
/**
 * 上游管理页：列表分页 + 新建/编辑（含后端节点编辑）+ 删除。
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
import AppInput from '@/components/ui/AppInput.vue'
import AppModal from '@/components/ui/AppModal.vue'
import AppPagination from '@/components/ui/AppPagination.vue'
import AppSelect from '@/components/ui/AppSelect.vue'
import AppTable, { type TableColumn } from '@/components/ui/AppTable.vue'
import { BalancerAlgorithm, BalancerAlgorithmLabels, toOptions } from '@/types/enums'
import type {
  CreateUpstreamNodePayload,
  CreateUpstreamPayload,
  UpdateUpstreamPayload,
  UpstreamDto,
} from '@/types'

// ── 状态 ──

const auth = useAuthStore()
const toast = useToastStore()

const upstreams = ref<UpstreamDto[]>([])
const total = ref(0)
const page = ref(1)
const pageSize = 20
const loading = ref(false)

const formOpen = ref(false)
const editingId = ref<number | null>(null)
const saving = ref(false)
const deleting = ref<UpstreamDto | null>(null)
const deletingLoading = ref(false)

// ── 表单模型 ──

interface NodeForm {
  address: string
  weight: number
  enabled: boolean
}

interface UpstreamForm {
  name: string
  algorithm: BalancerAlgorithm
  retryEnabled: boolean
  nodes: NodeForm[]
}

function emptyForm(): UpstreamForm {
  return {
    name: '',
    algorithm: BalancerAlgorithm.RoundRobin,
    retryEnabled: false,
    nodes: [{ address: '', weight: 1, enabled: true }],
  }
}

const form = ref<UpstreamForm>(emptyForm())

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
  editingId.value = null
  form.value = emptyForm()
  formOpen.value = true
}

function openEdit(row: UpstreamDto): void {
  editingId.value = row.id
  form.value = {
    name: row.name,
    algorithm: row.algorithm,
    retryEnabled: row.retry_enabled,
    nodes: row.nodes.map((n) => ({ address: n.address, weight: n.weight, enabled: n.enabled })),
  }
  formOpen.value = true
}

function addNode(): void {
  form.value.nodes.push({ address: '', weight: 1, enabled: true })
}

function removeNode(index: number): void {
  form.value.nodes.splice(index, 1)
}

async function save(): Promise<void> {
  if (!form.value.name.trim()) {
    toast.error('请填写上游名称')
    return
  }
  const validNodes = form.value.nodes.filter((n) => n.address.trim())
  if (validNodes.length === 0) {
    toast.error('至少需要一个后端节点地址')
    return
  }
  const nodes: CreateUpstreamNodePayload[] = validNodes.map((n) => ({
    address: n.address.trim(),
    weight: n.weight,
    enabled: n.enabled,
  }))

  saving.value = true
  try {
    if (editingId.value === null) {
      const payload: CreateUpstreamPayload = {
        name: form.value.name,
        algorithm: form.value.algorithm,
        retry_enabled: form.value.retryEnabled,
        nodes,
      }
      await upstreamApi.create(payload)
      toast.success('上游创建成功')
    } else {
      const payload: UpdateUpstreamPayload = {
        id: editingId.value,
        name: form.value.name,
        algorithm: form.value.algorithm,
        retry_enabled: form.value.retryEnabled,
        nodes,
      }
      await upstreamApi.update(payload)
      toast.success('上游已更新')
    }
    formOpen.value = false
    await load()
  } catch (e) {
    toast.error((e as Error).message)
  } finally {
    saving.value = false
  }
}

async function confirmDelete(): Promise<void> {
  if (!deleting.value) return
  deletingLoading.value = true
  try {
    await upstreamApi.remove(deleting.value.id)
    toast.success(`已删除上游「${deleting.value.name}」`)
    deleting.value = null
    if (upstreams.value.length === 1 && page.value > 1) page.value -= 1
    await load()
  } catch (e) {
    toast.error((e as Error).message)
  } finally {
    deletingLoading.value = false
  }
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
  <AppModal :open="formOpen" :title="editingId === null ? '新建上游' : `编辑上游 #${editingId}`" :width="'max-w-2xl'" @close="formOpen = false">
    <form class="space-y-4" @submit.prevent="save">
      <div class="grid grid-cols-3 gap-4">
        <AppInput v-model="form.name" label="上游名称" required placeholder="例如 product-api" />
        <AppSelect v-model="form.algorithm" label="负载均衡算法" :options="toOptions(BalancerAlgorithmLabels)" />
        <label class="flex items-end pb-2 text-sm text-slate-600">
          <input v-model="form.retryEnabled" type="checkbox" class="mr-1.5 accent-indigo-600" />
          失败自动重试
        </label>
      </div>

      <!-- 节点编辑器 -->
      <div>
        <div class="mb-2 flex items-center justify-between">
          <span class="text-sm font-medium text-slate-700">后端节点</span>
          <AppButton variant="secondary" size="sm" @click="addNode">添加节点</AppButton>
        </div>
        <div class="space-y-2">
          <div
            v-for="(node, index) in form.nodes"
            :key="index"
            class="grid grid-cols-[1fr_90px_70px_32px] items-center gap-2"
          >
            <AppInput v-model="node.address" placeholder="host:port，例如 127.0.0.1:9090" />
            <AppInput v-model.number="node.weight" label="" type="number" placeholder="权重" />
            <label class="flex items-center gap-1 text-xs text-slate-600">
              <input v-model="node.enabled" type="checkbox" class="accent-indigo-600" />
              启用
            </label>
            <AppButton
              variant="ghost"
              size="sm"
              class="text-red-500 hover:bg-red-50"
              @click="removeNode(index)"
            >
              删
            </AppButton>
          </div>
        </div>
      </div>
    </form>

    <template #footer>
      <AppButton variant="secondary" @click="formOpen = false">取消</AppButton>
      <AppButton :loading="saving" @click="save">保存</AppButton>
    </template>
  </AppModal>

  <!-- 删除确认 -->
  <AppModal :open="deleting !== null" title="删除上游" @close="deleting = null">
    <p class="text-sm text-slate-600">
      确定删除上游 <span class="font-medium text-slate-800">{{ deleting?.name }}</span> 吗？其下节点将一并失效。
    </p>
    <template #footer>
      <AppButton variant="secondary" @click="deleting = null">取消</AppButton>
      <AppButton variant="danger" :loading="deletingLoading" @click="confirmDelete">删除</AppButton>
    </template>
  </AppModal>
</template>
