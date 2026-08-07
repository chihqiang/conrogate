<script setup lang="ts">
/**
 * 路由管理页：列表分页 + 新建/编辑（模态框）+ 启用切换 + 删除。
 * 对应控制面接口见 docs/api.md「4. 路由管理」。
 */
import { computed, onMounted, ref } from 'vue'
import { routeApi } from '@/api/routes'
import { pluginApi } from '@/api/plugins'
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
  InstalledPluginDto,
  PathMatchUnion,
  PluginBindingDto,
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

/** 插件绑定弹窗状态 */
const bindingRoute = ref<RouteDto | null>(null)
const bindings = ref<PluginBindingDto[]>([])
const bindingsLoading = ref(false)
const installedPlugins = ref<InstalledPluginDto[]>([])

/** 绑定 / 编辑子弹窗 */
const bindFormOpen = ref(false)
const editingBindingName = ref<string | null>(null)
const bindSaving = ref(false)

/** 解绑确认 */
const unbinding = ref<PluginBindingDto | null>(null)
const unbindLoading = ref(false)

interface BindingForm {
  pluginName: string
  configText: string
  order: number
  blocking: boolean
  enabled: boolean
}

/** 官方插件默认配置模板（与后端插件 Default 对齐） */
const configTemplates: Record<string, string> = {
  log: '{\n  "log_body": false,\n  "log_headers": false,\n  "skip_paths": ["/healthz", "/readyz"]\n}',
  cors: '{\n  "allow_origins": ["*"],\n  "allow_methods": ["GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"],\n  "allow_headers": ["Content-Type", "Authorization"],\n  "expose_headers": [],\n  "allow_credentials": false,\n  "max_age_seconds": 3600\n}',
  auth: '{\n  "algorithm": "HS256",\n  "secret": "",\n  "require_token": true\n}',
}

const bindingForm = ref<BindingForm>({ pluginName: '', configText: '{}', order: 0, blocking: false, enabled: true })

/** JSON 配置实时校验：合法 → null，非法 → 错误文案 */
const configError = computed<string | null>(() => {
  const text = bindingForm.value.configText.trim()
  if (!text) return null
  try {
    const v = JSON.parse(text) as unknown
    if (v === null) return null
    if (typeof v !== 'object' || Array.isArray(v)) return '配置必须是 JSON 对象'
    return null
  } catch {
    return 'JSON 格式不正确'
  }
})

/** 美化格式化当前配置文本 */
function formatConfig(): void {
  const text = bindingForm.value.configText.trim()
  if (!text) return
  try {
    bindingForm.value.configText = JSON.stringify(JSON.parse(text), null, 2)
  } catch {
    toast.error('JSON 格式不正确，无法格式化')
  }
}

/** 重置为所选插件的默认配置模板（仅新建时） */
function resetConfigTemplate(): void {
  const name = bindingForm.value.pluginName
  bindingForm.value.configText = configTemplates[name] ?? '{}'
}

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
  { key: 'actions', label: '操作', width: '260px', align: 'right' },
]

/** 插件绑定表格列 */
const bindingColumns: TableColumn[] = [
  { key: 'plugin_name', label: '插件', width: '120px' },
  { key: 'order', label: '顺序', width: '60px' },
  { key: 'flags', label: '标志' },
  { key: 'config', label: '配置' },
  { key: 'actions', label: '操作', width: '140px', align: 'right' },
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

// ── 插件绑定 ──

/** 打开某路由的插件绑定管理弹窗 */
async function openBindings(row: RouteDto): Promise<void> {
  bindingRoute.value = row
  bindingsLoading.value = true
  try {
    bindings.value = await pluginApi.bindings(row.id)
  } catch (e) {
    toast.error((e as Error).message)
  } finally {
    bindingsLoading.value = false
  }
  if (installedPlugins.value.length === 0) {
    try {
      installedPlugins.value = await pluginApi.list()
    } catch {
      // 插件列表加载失败不影响绑定查看
    }
  }
}

function closeBindings(): void {
  bindingRoute.value = null
  bindings.value = []
}

/** 配置文本 → JSON 对象；空串为 null，非法 JSON 抛错 */
function parseConfig(text: string): Record<string, unknown> | null {
  const trimmed = text.trim()
  if (!trimmed) return null
  try {
    const v = JSON.parse(trimmed) as unknown
    if (v === null) return null
    if (typeof v !== 'object' || Array.isArray(v)) throw new Error('配置必须是 JSON 对象')
    return v as Record<string, unknown>
  } catch (e) {
    throw new Error(`插件配置不合法：${(e as Error).message}`)
  }
}

function emptyBindingForm(): BindingForm {
  return { pluginName: '', configText: '{}', order: 0, blocking: false, enabled: true }
}

function openBindForm(): void {
  editingBindingName.value = null
  bindingForm.value = emptyBindingForm()
  bindFormOpen.value = true
}

function openEditBinding(binding: PluginBindingDto): void {
  editingBindingName.value = binding.plugin_name
  bindingForm.value = {
    pluginName: binding.plugin_name,
    configText: JSON.stringify(binding.config ?? {}, null, 2),
    order: binding.order,
    blocking: binding.blocking,
    enabled: binding.enabled,
  }
  bindFormOpen.value = true
}

/** 新建绑定时切换插件 → 填入对应配置模板 */
function onBindingPluginChange(name: string): void {
  if (editingBindingName.value === null) {
    bindingForm.value.configText = configTemplates[name] ?? '{}'
  }
}

async function loadBindings(): Promise<void> {
  const routeId = bindingRoute.value?.id
  if (!routeId) return
  bindingsLoading.value = true
  try {
    bindings.value = await pluginApi.bindings(routeId)
  } catch (e) {
    toast.error((e as Error).message)
  } finally {
    bindingsLoading.value = false
  }
}

async function saveBinding(): Promise<void> {
  const routeId = bindingRoute.value?.id
  if (!routeId) return
  if (!bindingForm.value.pluginName) {
    toast.error('请选择插件')
    return
  }
  let config: Record<string, unknown> | null
  try {
    config = parseConfig(bindingForm.value.configText)
  } catch (e) {
    toast.error((e as Error).message)
    return
  }
  bindSaving.value = true
  try {
    if (editingBindingName.value === null) {
      await pluginApi.bind(routeId, {
        plugin_name: bindingForm.value.pluginName,
        config,
        order: bindingForm.value.order,
        blocking: bindingForm.value.blocking,
        enabled: bindingForm.value.enabled,
      })
      toast.success('插件绑定成功')
    } else {
      await pluginApi.updateBinding(routeId, editingBindingName.value, {
        config,
        order: bindingForm.value.order,
        blocking: bindingForm.value.blocking,
        enabled: bindingForm.value.enabled,
      })
      toast.success('插件绑定已更新')
    }
    bindFormOpen.value = false
    await loadBindings()
  } catch (e) {
    toast.error((e as Error).message)
  } finally {
    bindSaving.value = false
  }
}

async function confirmUnbind(): Promise<void> {
  const routeId = bindingRoute.value?.id
  if (!routeId || !unbinding.value) return
  unbindLoading.value = true
  try {
    await pluginApi.unbind(routeId, unbinding.value.plugin_name)
    toast.success(`已解绑插件「${unbinding.value.plugin_name}」`)
    unbinding.value = null
    await loadBindings()
  } catch (e) {
    toast.error((e as Error).message)
  } finally {
    unbindLoading.value = false
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
          <AppButton v-if="auth.canWrite" variant="ghost" size="sm" @click="openBindings(row as RouteDto)">
            插件
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

  <!-- 插件绑定管理 -->
  <AppModal
    :open="bindingRoute !== null"
    :title="`插件绑定 · ${bindingRoute?.name ?? ''}`"
    width="max-w-3xl"
    @close="closeBindings"
  >
    <AppTable :columns="bindingColumns" :rows="bindings" :loading="bindingsLoading">
      <template #cell-flags="{ row }">
        <div class="flex gap-1">
          <AppBadge :tone="(row as PluginBindingDto).enabled ? 'green' : 'gray'">
            {{ (row as PluginBindingDto).enabled ? '启用' : '停用' }}
          </AppBadge>
          <AppBadge :tone="(row as PluginBindingDto).blocking ? 'yellow' : 'blue'">
            {{ (row as PluginBindingDto).blocking ? '阻断' : '放行' }}
          </AppBadge>
        </div>
      </template>
      <template #cell-config="{ row }">
        <code class="block max-w-xs truncate font-mono text-xs text-slate-500">
          {{ JSON.stringify((row as PluginBindingDto).config ?? null) }}
        </code>
      </template>
      <template #cell-actions="{ row }">
        <div class="flex items-center justify-end gap-1">
          <AppButton v-if="auth.canWrite" variant="ghost" size="sm" @click="openEditBinding(row as PluginBindingDto)">
            编辑
          </AppButton>
          <AppButton v-if="auth.canWrite" variant="danger" size="sm" @click="unbinding = row as PluginBindingDto">
            解绑
          </AppButton>
        </div>
      </template>
      <template #empty>
        <AppEmpty text="该路由尚未绑定插件，点击右下角「绑定插件」添加" />
      </template>
    </AppTable>

    <template #footer>
      <AppButton variant="secondary" @click="closeBindings">关闭</AppButton>
      <AppButton v-if="auth.canWrite" @click="openBindForm">绑定插件</AppButton>
    </template>
  </AppModal>

  <!-- 绑定 / 编辑插件 -->
  <AppModal
    :open="bindFormOpen"
    :title="editingBindingName === null ? '绑定插件' : `编辑插件绑定 · ${editingBindingName}`"
    width="max-w-xl"
    @close="bindFormOpen = false"
  >
    <form class="space-y-5" @submit.prevent="saveBinding">
      <AppSelect
        v-model="bindingForm.pluginName"
        label="插件"
        :disabled="editingBindingName !== null"
        :options="installedPlugins.map((p) => ({ value: p.name, label: `${p.name}（${p.status}）` }))"
        placeholder="请选择插件"
        @change="onBindingPluginChange(bindingForm.pluginName)"
      />

      <div class="grid grid-cols-3 items-end gap-4">
        <AppInput v-model.number="bindingForm.order" label="执行顺序" type="number" />
        <div class="col-span-2 flex h-9 items-center gap-6">
          <label
            class="inline-flex cursor-pointer items-center gap-2 text-sm text-slate-600 transition hover:text-slate-900"
          >
            <input
              v-model="bindingForm.blocking"
              type="checkbox"
              class="h-4 w-4 rounded border-slate-300 accent-indigo-600"
            />
            阻断失败
          </label>
          <label
            class="inline-flex cursor-pointer items-center gap-2 text-sm text-slate-600 transition hover:text-slate-900"
          >
            <input
              v-model="bindingForm.enabled"
              type="checkbox"
              class="h-4 w-4 rounded border-slate-300 accent-indigo-600"
            />
            启用
          </label>
        </div>
      </div>

      <div>
        <div class="mb-1 flex items-center justify-between">
          <span class="text-sm font-medium text-slate-700">插件配置（JSON）</span>
          <div class="flex items-center gap-1">
            <button
              type="button"
              class="rounded px-2 py-0.5 text-xs text-indigo-600 transition hover:bg-indigo-50"
              @click="formatConfig"
            >
              格式化
            </button>
            <button
              v-if="editingBindingName === null"
              type="button"
              class="rounded px-2 py-0.5 text-xs text-slate-500 transition hover:bg-slate-100"
              @click="resetConfigTemplate"
            >
              重置模板
            </button>
          </div>
        </div>
        <textarea
          v-model="bindingForm.configText"
          rows="12"
          spellcheck="false"
          class="w-full resize-y rounded-md border bg-slate-50 p-2.5 font-mono text-xs leading-relaxed outline-none transition focus:ring-1"
          :class="
            configError
              ? 'border-red-400 focus:border-red-500 focus:ring-red-500'
              : 'border-slate-300 focus:border-indigo-500 focus:ring-indigo-500'
          "
          placeholder="{}"
        />
        <p v-if="configError" class="mt-1 flex items-center gap-1 text-xs text-red-500">
          <svg class="h-3.5 w-3.5" viewBox="0 0 20 20" fill="currentColor" aria-hidden="true">
            <path fill-rule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zM8.28 7.22a.75.75 0 00-1.06 1.06L8.94 10l-1.72 1.72a.75.75 0 101.06 1.06L10 11.06l1.72 1.72a.75.75 0 101.06-1.06L11.06 10l1.72-1.72a.75.75 0 00-1.06-1.06L10 8.94 8.28 7.22z" clip-rule="evenodd" />
          </svg>
          {{ configError }}
        </p>
        <p v-else-if="bindingForm.configText.trim()" class="mt-1 flex items-center gap-1 text-xs text-emerald-600">
          <svg class="h-3.5 w-3.5" viewBox="0 0 20 20" fill="currentColor" aria-hidden="true">
            <path fill-rule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zm3.857-9.809a.75.75 0 00-1.214-.882l-3.483 4.79-1.88-1.88a.75.75 0 10-1.06 1.061l2.5 2.5a.75.75 0 001.137-.089l4-5.5z" clip-rule="evenodd" />
          </svg>
          JSON 格式正确
        </p>
      </div>
    </form>

    <template #footer>
      <AppButton variant="secondary" @click="bindFormOpen = false">取消</AppButton>
      <AppButton :loading="bindSaving" :disabled="!!configError" @click="saveBinding">保存</AppButton>
    </template>
  </AppModal>

  <!-- 解绑确认 -->
  <AppModal :open="unbinding !== null" title="解绑插件" @close="unbinding = null">
    <p class="text-sm text-slate-600">
      确定从该路由解绑插件 <span class="font-medium text-slate-800">{{ unbinding?.plugin_name }}</span> 吗？
    </p>
    <template #footer>
      <AppButton variant="secondary" @click="unbinding = null">取消</AppButton>
      <AppButton variant="danger" :loading="unbindLoading" @click="confirmUnbind">解绑</AppButton>
    </template>
  </AppModal>
</template>
