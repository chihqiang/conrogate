<script setup lang="ts">
/**
 * 插件管理页：列出已安装插件，支持启用 / 停用 / 卸载（Admin 专属）。
 * 对应控制面接口：GET /plugins、POST /plugins/:name/{activate,disable}、DELETE /plugins/:name。
 */
import { onMounted, ref } from 'vue'
import { pluginApi } from '@/api/plugins'
import { useAuthStore } from '@/stores/auth'
import { useToastStore } from '@/stores/toast'
import AppBadge from '@/components/ui/AppBadge.vue'
import AppButton from '@/components/ui/AppButton.vue'
import AppCard from '@/components/ui/AppCard.vue'
import AppEmpty from '@/components/ui/AppEmpty.vue'
import AppModal from '@/components/ui/AppModal.vue'
import AppSelect from '@/components/ui/AppSelect.vue'
import AppTable, { type TableColumn } from '@/components/ui/AppTable.vue'
import { PluginKindLabels, PluginStatus, PluginStatusLabels, Role, toOptions } from '@/types/enums'
import type { InstalledPluginDto } from '@/types'

// ── 状态 ──

const auth = useAuthStore()
const toast = useToastStore()

const plugins = ref<InstalledPluginDto[]>([])
const loading = ref(false)
const statusFilter = ref('')

/** 操作按钮加载态（插件名 → 进行中的操作） */
const acting = ref<Record<string, string>>({})

/** 卸载确认 */
const uninstalling = ref<InstalledPluginDto | null>(null)
const uninstallLoading = ref(false)

// ── 辅助函数 ──

function fmtTime(value: string): string {
  if (!value) return '-'
  const d = new Date(value)
  if (Number.isNaN(d.getTime())) return '-'
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`
}

/** 插件清单标题/描述（后端写入的 manifest 元信息） */
function manifestText(p: InstalledPluginDto): string {
  const title = p.manifest?.title
  const desc = p.manifest?.description
  if (typeof title === 'string' && typeof desc === 'string') return `${title} · ${desc}`
  if (typeof title === 'string') return title
  return '-'
}

const isAdmin = auth.role === Role.Admin

/** 状态徽标色 */
function statusTone(status: string): 'green' | 'gray' | 'yellow' | 'blue' {
  if (status === PluginStatus.Active) return 'green'
  if (status === PluginStatus.Disabled) return 'gray'
  if (status === PluginStatus.Uninstalled) return 'blue'
  return 'yellow'
}

// ── 表格列 ──

const columns: TableColumn[] = [
  { key: 'name', label: '插件名', width: '120px' },
  { key: 'title', label: '说明' },
  { key: 'kind', label: '类型', width: '100px' },
  { key: 'status', label: '状态', width: '90px' },
  { key: 'version', label: '版本', width: '80px' },
  { key: 'installed_at', label: '安装时间', width: '150px', formatter: (v) => fmtTime(String(v)) },
  { key: 'actions', label: '操作', width: '200px', align: 'right' },
]

// ── 数据加载 ──

async function load(): Promise<void> {
  loading.value = true
  try {
    plugins.value = await pluginApi.list(statusFilter.value)
  } catch (e) {
    toast.error((e as Error).message)
  } finally {
    loading.value = false
  }
}

function onFilterChange(): void {
  void load()
}

// ── 操作 ──

async function runAction(p: InstalledPluginDto, action: 'activate' | 'disable'): Promise<void> {
  acting.value[p.name] = action
  try {
    if (action === 'activate') {
      await pluginApi.activate(p.name)
      toast.success(`已启用插件「${p.name}」`)
    } else {
      await pluginApi.disable(p.name)
      toast.success(`已停用插件「${p.name}」`)
    }
    await load()
  } catch (e) {
    toast.error((e as Error).message)
  } finally {
    delete acting.value[p.name]
  }
}

async function confirmUninstall(): Promise<void> {
  if (!uninstalling.value) return
  const name = uninstalling.value.name
  uninstallLoading.value = true
  try {
    await pluginApi.uninstall(name)
    toast.success(`已卸载插件「${name}」`)
    uninstalling.value = null
    await load()
  } catch (e) {
    toast.error((e as Error).message)
  } finally {
    uninstallLoading.value = false
  }
}

// ── 挂载 ──

onMounted(() => void load())
</script>

<template>
  <AppCard title="已安装插件">
    <template #actions>
      <AppSelect v-model="statusFilter" class="w-36" placeholder="全部状态" :options="toOptions(PluginStatusLabels)" @change="onFilterChange" />
      <AppButton size="sm" @click="load">刷新</AppButton>
    </template>

    <AppTable :columns="columns" :rows="plugins" :loading="loading">
      <!-- 说明 -->
      <template #cell-title="{ row }">
        <span class="text-slate-500">{{ manifestText(row as InstalledPluginDto) }}</span>
      </template>

      <!-- 类型 -->
      <template #cell-kind="{ value }">
        <AppBadge :tone="value === 'native' ? 'indigo' : 'yellow'">
          {{ PluginKindLabels[value as keyof typeof PluginKindLabels] ?? value }}
        </AppBadge>
      </template>

      <!-- 状态 -->
      <template #cell-status="{ value }">
        <AppBadge :tone="statusTone(String(value))">
          {{ PluginStatusLabels[value as PluginStatus] ?? value }}
        </AppBadge>
      </template>

      <!-- 操作 -->
      <template #cell-actions="{ row }">
        <div v-if="isAdmin" class="flex items-center justify-end gap-1">
          <AppButton
            v-if="(row as InstalledPluginDto).status === PluginStatus.Disabled || (row as InstalledPluginDto).status === PluginStatus.Installed"
            variant="ghost"
            size="sm"
            :loading="acting[(row as InstalledPluginDto).name] === 'activate'"
            @click="runAction(row as InstalledPluginDto, 'activate')"
          >
            启用
          </AppButton>
          <AppButton
            v-if="(row as InstalledPluginDto).status === PluginStatus.Active"
            variant="ghost"
            size="sm"
            :loading="acting[(row as InstalledPluginDto).name] === 'disable'"
            @click="runAction(row as InstalledPluginDto, 'disable')"
          >
            停用
          </AppButton>
          <AppButton
            v-if="(row as InstalledPluginDto).status !== PluginStatus.Uninstalled"
            variant="danger"
            size="sm"
            @click="uninstalling = row as InstalledPluginDto"
          >
            卸载
          </AppButton>
        </div>
        <span v-else class="text-xs text-slate-400">仅 Admin 可操作</span>
      </template>

      <template #empty>
        <AppEmpty text="暂无已安装插件。执行 conrogate-migrate --seed 注册官方插件（log / cors / auth）" />
      </template>
    </AppTable>
  </AppCard>

  <!-- 卸载确认 -->
  <AppModal :open="uninstalling !== null" title="卸载插件" @close="uninstalling = null">
    <p class="text-sm text-slate-600">
      确定卸载插件 <span class="font-medium text-slate-800">{{ uninstalling?.name }}</span> 吗？
      卸载后该插件将无法继续绑定到路由。
    </p>
    <template #footer>
      <AppButton variant="secondary" @click="uninstalling = null">取消</AppButton>
      <AppButton variant="danger" :loading="uninstallLoading" @click="confirmUninstall">卸载</AppButton>
    </template>
  </AppModal>
</template>
