<script setup lang="ts">
/**
 * 绑定 / 编辑插件弹窗（含 JSON 配置编辑器）。
 *
 * 用法：
 *   <RouteBindingFormModal v-model:open="open" :route-id="routeId" :editing="editing" :installed-plugins="plugins" @saved="reload" />
 */
import { computed, ref, watch } from 'vue'
import { pluginApi } from '@/api/plugins'
import { useToastStore } from '@/stores/toast'
import AppButton from '@/components/ui/AppButton.vue'
import AppInput from '@/components/ui/AppInput.vue'
import AppModal from '@/components/ui/AppModal.vue'
import AppSelect from '@/components/ui/AppSelect.vue'
import type { InstalledPluginDto, PluginBindingDto } from '@/types'

const props = defineProps<{
  open: boolean
  routeId: number
  /** 编辑目标；null 表示新建绑定 */
  editing: PluginBindingDto | null
  installedPlugins: InstalledPluginDto[]
}>()

const emit = defineEmits<{
  'update:open': [value: boolean]
  /** 保存成功（父级可在此刷新绑定列表） */
  saved: []
}>()

const toast = useToastStore()

interface BindingForm {
  pluginName: string
  configText: string
  order: number
  blocking: boolean
  enabled: boolean
}

function emptyForm(): BindingForm {
  return { pluginName: '', configText: '{}', order: 0, blocking: false, enabled: true }
}

/** 官方插件默认配置模板（与后端插件 Default 对齐） */
const configTemplates: Record<string, string> = {
  cors: '{\n  "allow_origins": ["*"],\n  "allow_methods": ["GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"],\n  "allow_headers": ["Content-Type", "Authorization"],\n  "expose_headers": [],\n  "allow_credentials": false,\n  "max_age_seconds": 3600\n}',
  auth: '{\n  "algorithm": "HS256",\n  "secret": "",\n  "require_token": true\n}',
  header_rewrite: '{\n  "request": {\n    "set": { "X-Real-IP": "$client_ip" },\n    "add": {},\n    "remove": []\n  },\n  "response": {\n    "set": { "X-Powered-By": "conrogate" },\n    "add": {},\n    "remove": []\n  }\n}',
  ip_allow_deny: '{\n  "allow": ["10.0.0.0/8", "192.168.1.0/24"],\n  "deny": ["10.20.0.0/16"]\n}',
}

const form = ref<BindingForm>(emptyForm())
const saving = ref(false)

/** JSON 配置实时校验：合法 → null，非法 → 错误文案 */
const configError = computed<string | null>(() => {
  const text = form.value.configText.trim()
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

// 每次打开时按编辑目标初始化表单
watch(
  () => props.open,
  (v) => {
    if (v) {
      form.value = props.editing
        ? {
            pluginName: props.editing.plugin_name,
            configText: JSON.stringify(props.editing.config ?? {}, null, 2),
            order: props.editing.order,
            blocking: props.editing.blocking,
            enabled: props.editing.enabled,
          }
        : emptyForm()
    }
  },
)

/** 美化格式化当前配置文本 */
function formatConfig(): void {
  const text = form.value.configText.trim()
  if (!text) return
  try {
    form.value.configText = JSON.stringify(JSON.parse(text), null, 2)
  } catch {
    toast.error('JSON 格式不正确，无法格式化')
  }
}

/** 重置为所选插件的默认配置模板（仅新建时） */
function resetConfigTemplate(): void {
  const name = form.value.pluginName
  form.value.configText = configTemplates[name] ?? '{}'
}

/** 新建绑定时切换插件 → 填入对应配置模板 */
function onPluginChange(name: string): void {
  if (props.editing === null) {
    form.value.configText = configTemplates[name] ?? '{}'
  }
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

async function save(): Promise<void> {
  if (!form.value.pluginName) {
    toast.error('请选择插件')
    return
  }
  let config: Record<string, unknown> | null
  try {
    config = parseConfig(form.value.configText)
  } catch (e) {
    toast.error((e as Error).message)
    return
  }
  saving.value = true
  try {
    if (props.editing === null) {
      await pluginApi.bind(props.routeId, {
        plugin_name: form.value.pluginName,
        config,
        order: form.value.order,
        blocking: form.value.blocking,
        enabled: form.value.enabled,
      })
      toast.success('插件绑定成功')
    } else {
      await pluginApi.updateBinding(props.routeId, props.editing.plugin_name, {
        config,
        order: form.value.order,
        blocking: form.value.blocking,
        enabled: form.value.enabled,
      })
      toast.success('插件绑定已更新')
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
    :title="editing === null ? '绑定插件' : `编辑插件绑定 · ${editing.plugin_name}`"
    width="max-w-xl"
    @close="emit('update:open', false)"
  >
    <form class="space-y-5" @submit.prevent="save">
      <AppSelect
        v-model="form.pluginName"
        label="插件"
        :disabled="editing !== null"
        :options="installedPlugins.map((p) => ({ value: p.name, label: `${p.name}（${p.status}）` }))"
        placeholder="请选择插件"
        @change="onPluginChange(form.pluginName)"
      />

      <div class="grid grid-cols-3 items-end gap-4">
        <AppInput v-model.number="form.order" label="执行顺序" type="number" />
        <div class="col-span-2 flex h-9 items-center gap-6">
          <label class="inline-flex cursor-pointer items-center gap-2 text-sm text-slate-600 transition hover:text-slate-900">
            <input
              v-model="form.blocking"
              type="checkbox"
              class="h-4 w-4 rounded border-slate-300 accent-indigo-600"
            />
            阻断失败
          </label>
          <label class="inline-flex cursor-pointer items-center gap-2 text-sm text-slate-600 transition hover:text-slate-900">
            <input
              v-model="form.enabled"
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
              v-if="editing === null"
              type="button"
              class="rounded px-2 py-0.5 text-xs text-slate-500 transition hover:bg-slate-100"
              @click="resetConfigTemplate"
            >
              重置模板
            </button>
          </div>
        </div>
        <textarea
          v-model="form.configText"
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
        <p v-else-if="form.configText.trim()" class="mt-1 flex items-center gap-1 text-xs text-emerald-600">
          <svg class="h-3.5 w-3.5" viewBox="0 0 20 20" fill="currentColor" aria-hidden="true">
            <path fill-rule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zm3.857-9.809a.75.75 0 00-1.214-.882l-3.483 4.79-1.88-1.88a.75.75 0 10-1.06 1.061l2.5 2.5a.75.75 0 001.137-.089l4-5.5z" clip-rule="evenodd" />
          </svg>
          JSON 格式正确
        </p>
      </div>
    </form>

    <template #footer>
      <AppButton variant="secondary" @click="emit('update:open', false)">取消</AppButton>
      <AppButton :loading="saving" :disabled="!!configError" @click="save">保存</AppButton>
    </template>
  </AppModal>
</template>
