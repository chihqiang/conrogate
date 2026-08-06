<script setup lang="ts">
/**
 * 路由管理页：列表分页 + 新建/编辑（模态框）+ 启用切换 + 删除。
 * 对应控制面接口见 docs/api.md「4. 路由管理」。
 */
import { computed, onMounted, ref } from 'vue'
import { routeApi } from '@/api/routes'
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
import {
  HttpMethod,
  HttpMethodLabels,
  PathMatchType,
  PathMatchTypeLabels,
  RouteProtocol,
  RouteProtocolLabels,
  toOptions,
} from '@/types/enums'
import type {
  CreateRoutePayload,
  PathMatchUnion,
  RouteDto,
  RouteMatchConditions,
  UpdateRoutePayload,
  UpstreamDto,
} from '@/types'

// ── 状态 ──

const auth = useAuthStore()
const toast = useToastStore()

const routes = ref<RouteDto[]>([])
const upstreams = ref<UpstreamDto[]>([])
const total = ref(0)
const page = ref(1)
const pageSize = 20
const loading = ref(false)

/** 新建/编辑弹窗是否打开 */
const formOpen = ref(false)
const editingId = ref<number | null>(null)
const saving = ref(false)

/** 删除确认 */
const deleting = ref<RouteDto | null>(null)
const deletingLoading = ref(false)

// 上游 id -> 名称 映射，用于表格显示
const upstreamNames = computed<Record<number, string>>(() =>
  Object.fromEntries(upstreams.value.map((u) => [u.id, u.name])),
)

// ── 表单模型 ──

interface RouteForm {
  name: string
  protocol: RouteProtocol
  pathType: PathMatchType
  pathValue: string
  methods: HttpMethod[]
  host: string
  upstreamId: number | null
  priority: number
  hostHeader: string
  allowRetry: boolean
  wsStripSensitive: boolean
  enabled: boolean
}

function emptyForm(): RouteForm {
  return {
    name: '',
    protocol: RouteProtocol.Http,
    pathType: PathMatchType.Prefix,
    pathValue: '/',
    methods: [],
    host: '',
    upstreamId: null,
    priority: 0,
    hostHeader: '',
    allowRetry: false,
    wsStripSensitive: false,
    enabled: true,
  }
}

const form = ref<RouteForm>(emptyForm())

// ── 辅助函数 ──

/** 路径匹配条件 → 展示文本，如 `前缀(prefix) /api` */
function pathText(path: PathMatchUnion): string {
  const entry = Object.entries(path)[0]
  if (!entry) return '-'
  const [type, value] = entry
  const label = PathMatchTypeLabels[type as PathMatchType] ?? type
  return `${label} ${value}`
}

/** 方法列表 → 展示文本；null/空 表示匹配全部 */
function methodsText(methods: string[] | null): string {
  return methods && methods.length > 0 ? methods.join(', ') : '全部'
}

/** 时间戳格式化（本地时区 YYYY-MM-DD HH:mm:ss） */
function fmtTime(value: string): string {
  if (!value) return '-'
  const d = new Date(value)
  if (Number.isNaN(d.getTime())) return '-'
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`
}

// ── 表格列定义 ──

const columns: TableColumn[] = [
  { key: 'id', label: 'ID', width: '56px' },
  { key: 'name', label: '名称' },
  { key: 'protocol', label: '协议', width: '110px' },
  { key: 'path', label: '路径', formatter: (_, row) => pathText((row as RouteDto).match_conditions.path) },
  { key: 'methods', label: '方法', width: '120px', formatter: (v) => methodsText(v as string[] | null) },
  { key: 'upstream_id', label: '上游', width: '120px', formatter: (v) => upstreamNames.value[v as number] ?? (v ? `#${v}` : '-') },
  { key: 'priority', label: '优先级', width: '70px' },
  { key: 'enabled', label: '状态', width: '80px' },
  { key: 'created_at', label: '创建时间', width: '150px', formatter: (v) => fmtTime(String(v)) },
  { key: 'actions', label: '操作', width: '200px', align: 'right' },
]

// ── 数据加载 ──

async function loadRoutes(): Promise<void> {
  loading.value = true
  try {
    const res = await routeApi.list({ page: page.value, page_size: pageSize })
    routes.value = res.list
    total.value = res.total
  } catch (e) {
    toast.error((e as Error).message)
  } finally {
    loading.value = false
  }
}

/** 加载上游列表（用于下拉与名称映射） */
async function loadUpstreams(): Promise<void> {
  try {
    const res = await upstreamApi.list({ page: 1, page_size: 100 })
    upstreams.value = res.list
  } catch {
    // 上游加载失败不影响路由列表展示
  }
}

// ── 新建 / 编辑 ──

function openCreate(): void {
  editingId.value = null
  form.value = emptyForm()
  formOpen.value = true
}

function openEdit(row: RouteDto): void {
  editingId.value = row.id
  const mc = row.match_conditions
  form.value = {
    name: row.name,
    protocol: row.protocol,
    pathType: Object.keys(mc.path)[0] as PathMatchType,
    pathValue: Object.values(mc.path)[0],
    methods: [...(mc.methods ?? [])] as HttpMethod[],
    host: mc.host ?? '',
    upstreamId: row.upstream_id,
    priority: row.priority,
    hostHeader: row.host_header ?? '',
    allowRetry: row.allow_retry_non_idempotent,
    wsStripSensitive: row.ws_strip_sensitive_headers,
    enabled: row.enabled,
  }
  formOpen.value = true
}

/** 组装 match_conditions 载荷 */
function buildMatchConditions(f: RouteForm): RouteMatchConditions {
  const path: PathMatchUnion = { [f.pathType]: f.pathValue } as PathMatchUnion
  return {
    path,
    methods: f.methods.length > 0 ? f.methods : null,
    host: f.host || null,
    headers: [],
    query_params: [],
  }
}

async function save(): Promise<void> {
  if (!form.value.name.trim()) {
    toast.error('请填写路由名称')
    return
  }
  if (!form.value.pathValue) {
    toast.error('请填写匹配路径')
    return
  }
  saving.value = true
  try {
    if (editingId.value === null) {
      const payload: CreateRoutePayload = {
        name: form.value.name,
        protocol: form.value.protocol,
        match_conditions: buildMatchConditions(form.value),
        priority: form.value.priority,
        upstream_id: form.value.upstreamId,
        host_header: form.value.hostHeader || null,
        allow_retry_non_idempotent: form.value.allowRetry,
        ws_strip_sensitive_headers: form.value.wsStripSensitive,
        enabled: form.value.enabled,
      }
      await routeApi.create(payload)
      toast.success('路由创建成功')
    } else {
      const payload: UpdateRoutePayload = {
        id: editingId.value,
        name: form.value.name,
        match_conditions: buildMatchConditions(form.value),
        priority: form.value.priority,
        upstream_id: form.value.upstreamId,
        host_header: form.value.hostHeader || null,
        allow_retry_non_idempotent: form.value.allowRetry,
        ws_strip_sensitive_headers: form.value.wsStripSensitive,
        enabled: form.value.enabled,
      }
      await routeApi.update(payload)
      toast.success('路由已更新')
    }
    formOpen.value = false
    await loadRoutes()
  } catch (e) {
    toast.error((e as Error).message)
  } finally {
    saving.value = false
  }
}

// ── 启用切换 / 删除 ──

async function toggleEnabled(row: RouteDto): Promise<void> {
  try {
    await routeApi.patch(row.id, { enabled: !row.enabled })
    toast.success(`已${row.enabled ? '停用' : '启用'}路由「${row.name}」`)
    await loadRoutes()
  } catch (e) {
    toast.error((e as Error).message)
  }
}

async function confirmDelete(): Promise<void> {
  if (!deleting.value) return
  deletingLoading.value = true
  try {
    await routeApi.remove(deleting.value.id)
    toast.success(`已删除路由「${deleting.value.name}」`)
    deleting.value = null
    // 当前页删空后回退一页
    if (routes.value.length === 1 && page.value > 1) page.value -= 1
    await loadRoutes()
  } catch (e) {
    toast.error((e as Error).message)
  } finally {
    deletingLoading.value = false
  }
}

// ── 挂载 ──

onMounted(() => {
  void loadRoutes()
  void loadUpstreams()
})

// ── 模板 ──
</script>

<template>
  <AppCard title="路由列表">
    <template #actions>
      <AppButton v-if="auth.canWrite" size="sm" @click="openCreate">新建路由</AppButton>
    </template>

    <AppTable :columns="columns" :rows="routes" :loading="loading">
      <!-- 协议 -->
      <template #cell-protocol="{ value }">
        <AppBadge :tone="value === 'http' ? 'blue' : value === 'web_socket' ? 'indigo' : 'yellow'">
          {{ RouteProtocolLabels[value as RouteProtocol] }}
        </AppBadge>
      </template>

      <!-- 启用状态 -->
      <template #cell-enabled="{ value }">
        <AppBadge :tone="value ? 'green' : 'gray'">{{ value ? '启用' : '停用' }}</AppBadge>
      </template>

      <!-- 操作列 -->
      <template #cell-actions="{ row }">
        <div class="flex items-center justify-end gap-1">
          <AppButton v-if="auth.canWrite" variant="ghost" size="sm" @click="toggleEnabled(row as RouteDto)">
            {{ (row as RouteDto).enabled ? '停用' : '启用' }}
          </AppButton>
          <AppButton v-if="auth.canWrite" variant="secondary" size="sm" @click="openEdit(row as RouteDto)">
            编辑
          </AppButton>
          <AppButton v-if="auth.canWrite" variant="danger" size="sm" @click="deleting = row as RouteDto">
            删除
          </AppButton>
        </div>
      </template>

      <template #empty>
        <AppEmpty text="暂无路由，点击右上角「新建路由」创建" />
      </template>
    </AppTable>

    <AppPagination :total="total" :page="page" :page-size="pageSize" @update:page="page = $event; loadRoutes()" />
  </AppCard>

  <!-- 新建 / 编辑弹窗 -->
  <AppModal :open="formOpen" :title="editingId === null ? '新建路由' : `编辑路由 #${editingId}`" @close="formOpen = false">
    <form class="space-y-4" @submit.prevent="save">
      <div class="grid grid-cols-2 gap-4">
        <AppInput v-model="form.name" label="路由名称" required placeholder="例如 product-api" />
        <AppSelect v-model="form.protocol" label="协议" :options="toOptions(RouteProtocolLabels)" />
      </div>

      <div class="grid grid-cols-2 gap-4">
        <AppSelect v-model="form.pathType" label="路径匹配方式" :options="toOptions(PathMatchTypeLabels)" />
        <AppInput v-model="form.pathValue" label="匹配路径" required placeholder="例如 /api" />
      </div>

      <div>
        <span class="mb-1 block text-sm font-medium text-slate-700">HTTP 方法（不选表示全部）</span>
        <div class="flex flex-wrap gap-2">
          <label
            v-for="m in toOptions(HttpMethodLabels)"
            :key="m.value"
            class="inline-flex cursor-pointer items-center gap-1 rounded border border-slate-300 px-2 py-1 text-xs"
            :class="form.methods.includes(m.value as HttpMethod) ? 'border-indigo-500 bg-indigo-50 text-indigo-700' : 'text-slate-600'"
          >
            <input v-model="form.methods" type="checkbox" :value="m.value" class="accent-indigo-600" />
            {{ m.label }}
          </label>
        </div>
      </div>

      <div class="grid grid-cols-2 gap-4">
        <AppSelect
          v-model="form.upstreamId"
          label="转发上游"
          :options="upstreams.map((u) => ({ value: u.id, label: `#${u.id} ${u.name}` }))"
        />
        <AppInput v-model="form.priority" label="优先级（越大越优先）" type="number" />
      </div>

      <div class="grid grid-cols-2 gap-4">
        <AppInput v-model="form.host" label="匹配 Host" placeholder="可留空" />
        <AppInput v-model="form.hostHeader" label="上游 Host 头覆盖" placeholder="可留空" />
      </div>

      <div class="flex flex-wrap gap-4 text-sm text-slate-600">
        <label class="inline-flex items-center gap-1.5">
          <input v-model="form.allowRetry" type="checkbox" class="accent-indigo-600" />
          允许非幂等重试
        </label>
        <label class="inline-flex items-center gap-1.5">
          <input v-model="form.wsStripSensitive" type="checkbox" class="accent-indigo-600" />
          WS 剥离敏感头
        </label>
        <label class="inline-flex items-center gap-1.5">
          <input v-model="form.enabled" type="checkbox" class="accent-indigo-600" />
          立即启用
        </label>
      </div>
    </form>

    <template #footer>
      <AppButton variant="secondary" @click="formOpen = false">取消</AppButton>
      <AppButton :loading="saving" @click="save">保存</AppButton>
    </template>
  </AppModal>

  <!-- 删除确认 -->
  <AppModal :open="deleting !== null" title="删除路由" @close="deleting = null">
    <p class="text-sm text-slate-600">
      确定删除路由 <span class="font-medium text-slate-800">{{ deleting?.name }}</span> 吗？该操作不可恢复。
    </p>
    <template #footer>
      <AppButton variant="secondary" @click="deleting = null">取消</AppButton>
      <AppButton variant="danger" :loading="deletingLoading" @click="confirmDelete">删除</AppButton>
    </template>
  </AppModal>
</template>
