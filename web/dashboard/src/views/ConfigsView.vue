<script setup lang="ts">
/**
 * 配置版本页：版本历史 + 发布新版本 + 回滚 + 版本差异对比。
 * 对应控制面接口见 docs/api.md「6. 配置版本管理」。
 */
import { onMounted, ref } from 'vue'
import { configApi } from '@/api/configs'
import { useAuthStore } from '@/stores/auth'
import { useToastStore } from '@/stores/toast'
import AppBadge from '@/components/ui/AppBadge.vue'
import AppButton from '@/components/ui/AppButton.vue'
import AppCard from '@/components/ui/AppCard.vue'
import AppEmpty from '@/components/ui/AppEmpty.vue'
import AppPagination from '@/components/ui/AppPagination.vue'
import AppTable, { type TableColumn } from '@/components/ui/AppTable.vue'
import ConfigDiffModal from '@/components/features/ConfigDiffModal.vue'
import ConfigRollbackModal from '@/components/features/ConfigRollbackModal.vue'
import PublishConfigButton from '@/components/features/PublishConfigButton.vue'
import { PublishType, PublishTypeLabels } from '@/types/enums'
import type { ConfigVersionDto } from '@/types'

// ── 状态 ──

const auth = useAuthStore()
const toast = useToastStore()

const versions = ref<ConfigVersionDto[]>([])
const total = ref(0)
const page = ref(1)
const pageSize = 20
const loading = ref(false)

// 回滚
const rollbackTarget = ref<ConfigVersionDto | null>(null)

// 差异对比
const diffOpen = ref(false)

// ── 辅助函数 ──

function fmtTime(value: string): string {
  if (!value) return '-'
  const d = new Date(value)
  if (Number.isNaN(d.getTime())) return '-'
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`
}

// ── 表格列定义 ──

const columns: TableColumn[] = [
  { key: 'version', label: '版本', width: '70px' },
  { key: 'publish_type', label: '类型', width: '80px' },
  { key: 'remark', label: '备注' },
  { key: 'base_version', label: '基准版本', width: '90px' },
  { key: 'created_by', label: '操作人', width: '110px', formatter: (v) => String(v ?? '-') },
  { key: 'applied_count', label: '已应用节点', width: '100px' },
  { key: 'created_at', label: '发布时间', width: '150px', formatter: (v) => fmtTime(String(v)) },
  { key: 'actions', label: '操作', width: '150px', align: 'right' },
]

// ── 数据加载 ──

async function load(): Promise<void> {
  loading.value = true
  try {
    const res = await configApi.versions({ page: page.value, page_size: pageSize })
    versions.value = res.list
    total.value = res.total
  } catch (e) {
    toast.error((e as Error).message)
  } finally {
    loading.value = false
  }
}

// ── 挂载 ──

onMounted(() => void load())
</script>

<template>
  <AppCard title="配置版本历史">
    <template #actions>
      <AppButton v-if="auth.canWrite" variant="secondary" size="sm" @click="diffOpen = true">版本对比</AppButton>
      <PublishConfigButton @published="load" />
    </template>

    <AppTable :columns="columns" :rows="versions" :loading="loading">
      <!-- 发布类型 -->
      <template #cell-publish_type="{ value }">
        <AppBadge :tone="value === PublishType.Rollback ? 'yellow' : 'indigo'">
          {{ PublishTypeLabels[value as PublishType] }}
        </AppBadge>
      </template>

      <!-- 操作列 -->
      <template #cell-actions="{ row }">
        <div class="flex items-center justify-end gap-1">
          <AppButton v-if="auth.canWrite" variant="secondary" size="sm" @click="rollbackTarget = row as ConfigVersionDto">
            回滚
          </AppButton>
          <AppButton v-if="auth.canWrite" variant="ghost" size="sm" @click="diffOpen = true">对比</AppButton>
        </div>
      </template>

      <template #empty>
        <AppEmpty text="暂无版本记录，点击右上角「发布配置」生成首个版本" />
      </template>
    </AppTable>

    <AppPagination :total="total" :page="page" :page-size="pageSize" @update:page="page = $event; load()" />
  </AppCard>

  <!-- 回滚确认 -->
  <ConfigRollbackModal :version="rollbackTarget" @close="rollbackTarget = null" @rolledback="load" />

  <!-- 版本对比弹窗 -->
  <ConfigDiffModal v-model:open="diffOpen" :versions="versions" />
</template>
