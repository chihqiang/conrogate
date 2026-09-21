<script setup lang="ts">
/**
 * 安全中心页：全局 IP 黑名单管理。
 * 拉黑（IP / CIDR / 时长）/ 解拉黑，写操作需 Operator+，读操作 Viewer。
 * 对应控制面接口见 docs/api.md「11.5 IP 黑名单管理」。
 */
import { onMounted, ref } from 'vue'
import { securityApi } from '@/api/security'
import { useAuthStore } from '@/stores/auth'
import { useToastStore } from '@/stores/toast'
import BlacklistCreateModal from '@/components/features/BlacklistCreateModal.vue'
import BlacklistRemoveModal from '@/components/features/BlacklistRemoveModal.vue'
import AppBadge from '@/components/ui/AppBadge.vue'
import AppButton from '@/components/ui/AppButton.vue'
import AppCard from '@/components/ui/AppCard.vue'
import AppEmpty from '@/components/ui/AppEmpty.vue'
import AppInput from '@/components/ui/AppInput.vue'
import AppPagination from '@/components/ui/AppPagination.vue'
import AppTable, { type TableColumn } from '@/components/ui/AppTable.vue'
import type { IpBlacklistDto } from '@/types'

// ── 状态 ──

const auth = useAuthStore()
const toast = useToastStore()
const canWrite = auth.canWrite

const items = ref<IpBlacklistDto[]>([])
const total = ref(0)
const page = ref(1)
const pageSize = 20
const loading = ref(false)

const keyword = ref('')

/** 待解除条目确认 */
const removing = ref<IpBlacklistDto | null>(null)

// ── 拉黑弹窗开关 ──
const creating = ref(false)

// ── 辅助函数 ──

function fmtTime(value: string | null): string {
  if (!value) return '永久'
  const d = new Date(value)
  if (Number.isNaN(d.getTime())) return '永久'
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`
}

function fmtExpires(row: IpBlacklistDto): string {
  if (!row.expires_at) return '永久'
  const left = Date.parse(row.expires_at) - Date.now()
  if (left <= 0) return '已过期'
  const secs = Math.floor(left / 1000)
  if (secs >= 86400) return `剩余 ${Math.floor(secs / 86400)} 天`
  if (secs >= 3600) return `剩余 ${Math.floor(secs / 3600)} 小时`
  return `剩余 ${Math.max(1, Math.floor(secs / 60))} 分钟`
}

function isExpired(row: IpBlacklistDto): boolean {
  return !!row.expires_at && Date.parse(row.expires_at) <= Date.now()
}

// ── 表格列 ──

const columns: TableColumn[] = [
  { key: 'ip_or_cidr', label: 'IP / 网段', width: '180px' },
  { key: 'reason', label: '原因' },
  { key: 'expires_at', label: '过期时间', width: '160px', formatter: (v) => fmtTime(String(v ?? '')) },
  { key: 'created_by', label: '操作人', width: '110px', formatter: (v) => String(v ?? '-') },
  { key: 'created_at', label: '拉黑时间', width: '150px', formatter: (v) => fmtTime(String(v)) },
  { key: 'actions', label: '操作', width: '90px', align: 'right' },
]

// ── 数据加载 ──

async function load(): Promise<void> {
  loading.value = true
  try {
    const res = await securityApi.list({
      page: page.value,
      page_size: pageSize,
      keyword: keyword.value.trim() || undefined,
    })
    items.value = res.list
    total.value = res.total
  } catch (e) {
    toast.error((e as Error).message)
  } finally {
    loading.value = false
  }
}

function search(): void {
  page.value = 1
  void load()
}

function reset(): void {
  keyword.value = ''
  page.value = 1
  void load()
}

// ── 拉黑 ──

function openCreate(): void {
  creating.value = true
}

// ── 解拉黑 ──

// ── 挂载 ──

onMounted(() => void load())
</script>

<template>
  <AppCard title="全局 IP 黑名单">
    <template #actions>
      <div class="flex items-end gap-2">
        <AppInput v-model="keyword" placeholder="搜索 IP / 网段 / 原因" @keyup.enter="search" />
        <AppButton size="sm" variant="secondary" @click="search">查询</AppButton>
        <AppButton size="sm" variant="secondary" @click="reset">重置</AppButton>
        <AppButton v-if="canWrite" size="sm" @click="openCreate">拉黑</AppButton>
      </div>
    </template>

    <AppTable :columns="columns" :rows="items" :loading="loading">
      <!-- IP / 网段 -->
      <template #cell-ip_or_cidr="{ value }">
        <code class="rounded bg-slate-100 px-1.5 py-0.5 text-xs font-medium text-slate-700">{{ value }}</code>
      </template>

      <!-- 原因 -->
      <template #cell-reason="{ value }">
        <span class="text-slate-600">{{ value || '-' }}</span>
      </template>

      <!-- 状态 -->
      <template #cell-expires_at="{ row }">
        <AppBadge :tone="isExpired(row as IpBlacklistDto) ? 'red' : (row as IpBlacklistDto).expires_at ? 'yellow' : 'gray'">
          {{ fmtExpires(row as IpBlacklistDto) }}
        </AppBadge>
      </template>

      <!-- 操作 -->
      <template #cell-actions="{ row }">
        <div v-if="canWrite" class="flex items-center justify-end gap-1">
          <AppButton variant="danger" size="sm" @click="removing = row as IpBlacklistDto">解除</AppButton>
        </div>
        <span v-else class="text-xs text-slate-400">只读</span>
      </template>

      <template #empty>
        <AppEmpty text="暂无拉黑条目。点击「拉黑」封禁 IP 或 CIDR 网段" />
      </template>
    </AppTable>

    <AppPagination :total="total" :page="page" :page-size="pageSize" @update:page="page = $event; load()" />
  </AppCard>

  <BlacklistCreateModal v-model:open="creating" @created="page = 1; load()" />

  <BlacklistRemoveModal :item="removing" @close="removing = null" @removed="load()" />
</template>
