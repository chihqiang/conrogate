<script setup lang="ts">
/**
 * 路由插件绑定管理弹窗：绑定列表 + 绑定/编辑（RouteBindingFormModal）+ 解绑（RouteUnbindModal）。
 * 自包含数据加载，父级仅需控制 route 开关。
 *
 * 用法：
 *   <RouteBindingsModal :route="bindingRoute" @close="bindingRoute = null" />
 */
import { ref, watch } from 'vue'
import { pluginApi } from '@/api/plugins'
import { useAuthStore } from '@/stores/auth'
import { useToastStore } from '@/stores/toast'
import AppBadge from '@/components/ui/AppBadge.vue'
import AppButton from '@/components/ui/AppButton.vue'
import AppEmpty from '@/components/ui/AppEmpty.vue'
import AppModal from '@/components/ui/AppModal.vue'
import AppTable, { type TableColumn } from '@/components/ui/AppTable.vue'
import RouteBindingFormModal from '@/components/features/RouteBindingFormModal.vue'
import RouteUnbindModal from '@/components/features/RouteUnbindModal.vue'
import type { InstalledPluginDto, PluginBindingDto, RouteDto } from '@/types'

const props = defineProps<{
  /** 目标路由；null 表示关闭 */
  route: RouteDto | null
}>()

const emit = defineEmits<{
  close: []
}>()

const auth = useAuthStore()
const toast = useToastStore()

const bindings = ref<PluginBindingDto[]>([])
const bindingsLoading = ref(false)
const installedPlugins = ref<InstalledPluginDto[]>([])

// 绑定 / 编辑子弹窗
const bindFormOpen = ref(false)
const editingBinding = ref<PluginBindingDto | null>(null)

// 解绑确认
const unbinding = ref<PluginBindingDto | null>(null)

/** 插件绑定表格列 */
const columns: TableColumn[] = [
  { key: 'plugin_name', label: '插件', width: '120px' },
  { key: 'order', label: '顺序', width: '60px' },
  { key: 'flags', label: '标志' },
  { key: 'config', label: '配置' },
  { key: 'actions', label: '操作', width: '140px', align: 'right' },
]

async function loadBindings(): Promise<void> {
  const routeId = props.route?.id
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

async function loadInstalledPlugins(): Promise<void> {
  if (installedPlugins.value.length > 0) return
  try {
    installedPlugins.value = await pluginApi.list()
  } catch {
    // 插件列表加载失败不影响绑定查看
  }
}

// 打开时加载绑定列表与插件清单
watch(
  () => props.route,
  (route) => {
    if (route) {
      void loadBindings()
      void loadInstalledPlugins()
    } else {
      bindings.value = []
      unbinding.value = null
    }
  },
  { immediate: true },
)

function closeBindings(): void {
  if (props.route) emit('close')
}

function openBindForm(): void {
  editingBinding.value = null
  bindFormOpen.value = true
}

function openEditBinding(binding: PluginBindingDto): void {
  editingBinding.value = binding
  bindFormOpen.value = true
}

function onBindingSaved(): void {
  void loadBindings()
}

function onUnbound(): void {
  void loadBindings()
}
</script>

<template>
  <AppModal :open="route !== null" :title="`插件绑定 · ${route?.name ?? ''}`" width="max-w-3xl" @close="closeBindings">
    <AppTable :columns="columns" :rows="bindings" :loading="bindingsLoading">
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

  <RouteBindingFormModal
    v-model:open="bindFormOpen"
    :route-id="route?.id ?? 0"
    :editing="editingBinding"
    :installed-plugins="installedPlugins"
    @saved="onBindingSaved"
  />

  <RouteUnbindModal
    :route-id="route?.id ?? 0"
    :binding="unbinding"
    @close="unbinding = null"
    @unbound="onUnbound"
  />
</template>
