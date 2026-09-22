<script setup lang="ts">
/**
 * 新建 / 编辑路由弹窗。
 * route 为 null 表示新建，否则为编辑目标。
 *
 * 用法：
 *   <RouteFormModal v-model:open="open" :route="editingRoute" :upstreams="upstreams" @saved="reload" />
 */
import { ref, watch } from 'vue'
import { routeApi } from '@/api/routes'
import { useToastStore } from '@/stores/toast'
import AppButton from '@/components/ui/AppButton.vue'
import AppInput from '@/components/ui/AppInput.vue'
import AppModal from '@/components/ui/AppModal.vue'
import AppSelect from '@/components/ui/AppSelect.vue'
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

const props = defineProps<{
  open: boolean
  /** null 表示新建，否则编辑该路由 */
  route: RouteDto | null
  /** 上游下拉选项 */
  upstreams: UpstreamDto[]
}>()

const emit = defineEmits<{
  'update:open': [value: boolean]
  /** 保存成功（父级可在此刷新列表） */
  saved: []
}>()

const toast = useToastStore()

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

function formFromRoute(row: RouteDto): RouteForm {
  const mc = row.match_conditions
  return {
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
}

const form = ref<RouteForm>(emptyForm())
const saving = ref(false)

// 每次打开时按编辑目标初始化表单
watch(
  () => props.open,
  (v) => {
    if (v) form.value = props.route ? formFromRoute(props.route) : emptyForm()
  },
)

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
    if (props.route === null) {
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
        id: props.route.id,
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
    emit('saved')
    emit('update:open', false)
  } catch (e) {
    toast.error((e as Error).message)
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <AppModal
    :open="open"
    :title="route === null ? '新建路由' : `编辑路由 #${route.id}`"
    width="max-w-2xl"
    @close="emit('update:open', false)"
  >
    <form class="space-y-5" @submit.prevent="save">
      <!-- 分区：基本信息 -->
      <div>
        <h4 class="mb-3 text-xs font-semibold uppercase tracking-wide text-slate-400">基本信息</h4>
        <div class="grid grid-cols-2 gap-4">
          <AppInput v-model="form.name" label="路由名称" required placeholder="例如 product-api" />
          <AppSelect v-model="form.protocol" label="协议" :options="toOptions(RouteProtocolLabels)" />
        </div>
      </div>

      <!-- 分区：匹配规则 -->
      <div>
        <h4 class="mb-3 text-xs font-semibold uppercase tracking-wide text-slate-400">匹配规则</h4>
        <div class="grid grid-cols-2 gap-4">
          <AppSelect v-model="form.pathType" label="路径匹配方式" :options="toOptions(PathMatchTypeLabels)" />
          <AppInput v-model="form.pathValue" label="匹配路径" required placeholder="例如 /api" hint="支持前缀、精确、正则三种匹配方式" />
        </div>

        <!-- HTTP 方法 -->
        <div class="mt-3">
          <span class="mb-1.5 block text-sm font-medium text-slate-700">HTTP 方法</span>
          <div class="flex flex-wrap gap-1.5">
            <label
              v-for="m in toOptions(HttpMethodLabels)"
              :key="m.value"
              class="inline-flex cursor-pointer items-center gap-1.5 rounded-lg border px-2.5 py-1 text-xs transition-all duration-150"
              :class="form.methods.includes(m.value as HttpMethod)
                ? 'border-indigo-500 bg-indigo-50 text-indigo-700 shadow-sm'
                : 'border-slate-300 text-slate-600 hover:border-slate-400 hover:bg-slate-50'"
            >
              <input v-model="form.methods" type="checkbox" :value="m.value" class="hidden accent-indigo-600" />
              {{ m.label }}
            </label>
            <span class="ml-1 inline-flex items-center text-xs text-slate-400">不选 = 全部方法</span>
          </div>
        </div>

        <!-- Host 匹配 -->
        <div class="mt-3 grid grid-cols-2 gap-4">
          <AppInput v-model="form.host" label="匹配 Host" placeholder="可留空" hint="如 api.example.com" />
          <AppInput v-model="form.hostHeader" label="上游 Host 头覆盖" placeholder="可留空" hint="重写转发给上游的 Host 头" />
        </div>
      </div>

      <!-- 分区：转发配置 -->
      <div>
        <h4 class="mb-3 text-xs font-semibold uppercase tracking-wide text-slate-400">转发配置</h4>
        <div class="grid grid-cols-2 gap-4">
          <AppSelect
            v-model="form.upstreamId"
            label="转发上游"
            allow-clear
            :options="upstreams.map((u) => ({ value: u.id, label: `#${u.id} ${u.name}` }))"
          />
          <AppInput v-model="form.priority" label="优先级" type="number" hint="数字越大优先级越高" />
        </div>
      </div>

      <!-- 分区：高级选项 -->
      <div>
        <h4 class="mb-3 text-xs font-semibold uppercase tracking-wide text-slate-400">高级选项</h4>
        <div class="grid grid-cols-3 gap-3">
          <label
            class="flex cursor-pointer items-center gap-2 rounded-lg border border-slate-300 p-2.5 text-sm transition-all duration-150 hover:border-slate-400 hover:bg-slate-50"
            :class="form.allowRetry ? 'border-indigo-500 bg-indigo-50' : ''"
          >
            <input v-model="form.allowRetry" type="checkbox" class="h-4 w-4 rounded accent-indigo-600" />
            <span :class="form.allowRetry ? 'text-indigo-700' : 'text-slate-600'">允许非幂等重试</span>
          </label>
          <label
            class="flex cursor-pointer items-center gap-2 rounded-lg border border-slate-300 p-2.5 text-sm transition-all duration-150 hover:border-slate-400 hover:bg-slate-50"
            :class="form.wsStripSensitive ? 'border-indigo-500 bg-indigo-50' : ''"
          >
            <input v-model="form.wsStripSensitive" type="checkbox" class="h-4 w-4 rounded accent-indigo-600" />
            <span :class="form.wsStripSensitive ? 'text-indigo-700' : 'text-slate-600'">WS 剥离敏感头</span>
          </label>
          <label
            class="flex cursor-pointer items-center gap-2 rounded-lg border border-slate-300 p-2.5 text-sm transition-all duration-150 hover:border-slate-400 hover:bg-slate-50"
            :class="form.enabled ? 'border-emerald-500 bg-emerald-50' : ''"
          >
            <input v-model="form.enabled" type="checkbox" class="h-4 w-4 rounded accent-indigo-600" />
            <span :class="form.enabled ? 'text-emerald-700' : 'text-slate-600'">立即启用</span>
          </label>
        </div>
      </div>
    </form>

    <template #footer>
      <AppButton variant="secondary" @click="emit('update:open', false)">取消</AppButton>
      <AppButton :loading="saving" @click="save">保存</AppButton>
    </template>
  </AppModal>
</template>
