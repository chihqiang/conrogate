<script setup lang="ts">
/**
 * 路由管理页：列表分页 + 新建/编辑（模态框）+ 启用切换 + 删除 + 插件绑定。
 * 对应控制面接口见 docs/api.md「4. 路由管理」。
 */
import { computed, onMounted, ref } from 'vue'
import { routeApi } from '@/api/routes'
import { upstreamApi } from '@/api/upstreams'
import { useAuthStore } from '@/stores/auth'
import { useToastStore } from '@/stores/toast'
import { fmtTime } from '@/utils/format'
import AppBadge from '@/components/ui/AppBadge.vue'
import AppButton from '@/components/ui/AppButton.vue'
import AppCard from '@/components/ui/AppCard.vue'
import AppEmpty from '@/components/ui/AppEmpty.vue'
import AppPagination from '@/components/ui/AppPagination.vue'
import AppTable, { type TableColumn } from '@/components/ui/AppTable.vue'
import RouteBindingsModal from '@/components/features/RouteBindingsModal.vue'
import RouteDeleteModal from '@/components/features/RouteDeleteModal.vue'
import RouteFormModal from '@/components/features/RouteFormModal.vue'
import PublishConfigButton from '@/components/features/PublishConfigButton.vue'
import { RouteProtocol, RouteProtocolLabels, PathMatchType, PathMatchTypeLabels } from '@/types/enums'
import type { RouteDto, UpstreamDto } from '@/types'

// ── 状态 ──

const auth = useAuthStore()
const toast = useToastStore()

const routes = ref<RouteDto[]>([])
const upstreams = ref<UpstreamDto[]>([])
const total = ref(0)
const page = ref(1)
const pageSize = 20
const loading = ref(false)

/** 新建/编辑弹窗：editingRoute 为 null 表示新建 */
const formOpen = ref(false)
const editingRoute = ref<RouteDto | null>(null)

/** 删除确认 */
const deleting = ref<RouteDto | null>(null)

/** 插件绑定弹窗 */
const bindingRoute = ref<RouteDto | null>(null)

// 上游 id -> 名称 映射，用于表格显示
const upstreamNames = computed<Record<number, string>>(() =>
  Object.fromEntries(upstreams.value.map((u) => [u.id, u.name])),
)

// ── 辅助函数 ──

/** 路径匹配条件 → 展示文本，如 `前缀(prefix) /api` */
function pathText(path: RouteDto['match_conditions']['path']): string {
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

/** 从路由生成 curl 访问示例 */
function curlPreview(row: RouteDto): string {
  const mc = row.match_conditions
  const pathEntry = Object.values(mc.path)[0] ?? '/'
  const method = mc.methods && mc.methods.length > 0 ? mc.methods[0] : 'GET'
  const host = mc.host || 'localhost:8080'
  return `curl -X ${method} http://${host}${pathEntry}`
}

/** 复制 curl 到剪贴板 */
async function copyCurl(row: RouteDto): Promise<void> {
  const text = curlPreview(row)
  try {
    await navigator.clipboard.writeText(text)
    toast.success(`已复制：${text}`)
  } catch {
    toast.error('复制失败，请手动选择文本复制')
  }
}

// ── 表格列定义 ──

const columns: TableColumn[] = [
  { key: 'id', label: 'ID', width: '56px' },
  { key: 'name', label: '名称' },
  { key: 'protocol', label: '协议', width: '100px' },
  { key: 'path', label: '路径', formatter: (_, row) => pathText((row as RouteDto).match_conditions.path) },
  { key: 'methods', label: '方法', width: '110px', formatter: (v) => methodsText(v as string[] | null) },
  { key: 'upstream_id', label: '上游', width: '110px', formatter: (v) => upstreamNames.value[v as number] ?? (v ? `#${v}` : '-') },
  { key: 'priority', label: '优先级', width: '70px' },
  { key: 'enabled', label: '状态', width: '70px' },
  { key: 'access', label: '访问示例', width: '200px' },
  { key: 'created_at', label: '创建时间', width: '150px', formatter: (v) => fmtTime(String(v)) },
  { key: 'actions', label: '操作', width: '230px', align: 'right' },
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
  editingRoute.value = null
  formOpen.value = true
}

function openEdit(row: RouteDto): void {
  editingRoute.value = row
  formOpen.value = true
}

function onRouteSaved(): void {
  void loadRoutes()
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

function onRouteDeleted(): void {
  // 当前页删空后回退一页
  if (routes.value.length === 1 && page.value > 1) page.value -= 1
  void loadRoutes()
}

// ── 插件绑定 ──

function openBindings(row: RouteDto): void {
  bindingRoute.value = row
}

// ── 挂载 ──

onMounted(() => {
  void loadRoutes()
  void loadUpstreams()
})
</script>

<template>
  <AppCard title="路由列表">
    <template #actions>
      <AppButton v-if="auth.canWrite" size="sm" @click="openCreate">新建路由</AppButton>
      <PublishConfigButton @published="loadRoutes" />
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

      <!-- 访问示例 -->
      <template #cell-access="{ row }">
        <div class="group flex items-center gap-1">
          <code class="block truncate rounded bg-slate-50 px-2 py-1 font-mono text-xs text-slate-600">
            {{ curlPreview(row as RouteDto) }}
          </code>
          <button
            class="shrink-0 rounded p-1 text-slate-400 transition hover:bg-indigo-50 hover:text-indigo-600"
            title="复制 curl"
            @click="copyCurl(row as RouteDto)"
          >
            <svg class="h-3.5 w-3.5" viewBox="0 0 20 20" fill="currentColor" aria-hidden="true">
              <path d="M8 2a2 2 0 00-2 2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2V4a2 2 0 00-2-2zM5 8a1 1 0 011-1h1v6H5a1 1 0 01-1-1V8z" />
              <path d="M12 6a2 2 0 012-2h1a2 2 0 012 2v6a2 2 0 01-2 2h-1a2 2 0 01-2-2V6zm3 0a1 1 0 00-1 1v6h1a1 1 0 001-1V6z" />
            </svg>
          </button>
        </div>
      </template>

      <!-- 操作列 -->
      <template #cell-actions="{ row }">
        <div class="flex items-center justify-end gap-1">
          <!-- 启用时中性（停用动作），停用时高亮（恢复流量动作） -->
          <AppButton
            v-if="auth.canWrite"
            :variant="(row as RouteDto).enabled ? 'secondary' : 'primary'"
            size="sm"
            @click="toggleEnabled(row as RouteDto)"
          >
            {{ (row as RouteDto).enabled ? '停用' : '启用' }}
          </AppButton>
          <AppButton v-if="auth.canWrite" variant="ghost" size="sm" @click="openBindings(row as RouteDto)">
            <svg
              class="h-3.5 w-3.5"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
              aria-hidden="true"
            >
              <path d="M12 2 2 7l10 5 10-5-10-5Z" />
              <path d="m2 17 10 5 10-5" />
              <path d="m2 12 10 5 10-5" />
            </svg>
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
  <RouteFormModal v-model:open="formOpen" :route="editingRoute" :upstreams="upstreams" @saved="onRouteSaved" />

  <!-- 删除确认 -->
  <RouteDeleteModal :route="deleting" @close="deleting = null" @deleted="onRouteDeleted" />

  <!-- 插件绑定管理（含绑定/编辑/解绑弹窗） -->
  <RouteBindingsModal :route="bindingRoute" @close="bindingRoute = null" />
</template>
