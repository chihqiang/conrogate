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
import AppBadge from '@/components/ui/AppBadge.vue'
import AppButton from '@/components/ui/AppButton.vue'
import AppCard from '@/components/ui/AppCard.vue'
import AppEmpty from '@/components/ui/AppEmpty.vue'
import AppInput from '@/components/ui/AppInput.vue'
import AppModal from '@/components/ui/AppModal.vue'
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

/** 待删除条目确认 */
const removing = ref<IpBlacklistDto | null>(null)

// ── 新建表单 ──
const creating = ref(false)
const formIp = ref('')
const formReason = ref('')
const formExpires = ref('')
const submitting = ref(false)

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

function fmtDuration(secs: number): string {
  if (secs >= 86400) return `${Math.floor(secs / 86400)} 天`
  if (secs >= 3600) return `${Math.floor(secs / 3600)} 小时`
  return `${secs} 秒`
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
  formIp.value = ''
  formReason.value = ''
  formExpires.value = ''
  creating.value = true
}

/** 解析时长输入（秒）；空串返回 null */
function parseExpires(raw: string): number | null {
  if (!raw.trim()) return null
  const n = Number(raw)
  return Number.isFinite(n) ? n : NaN
}

async function submitCreate(): Promise<void> {
  const ip = formIp.value.trim()
  if (!ip) {
    toast.error('请输入要拉黑的 IP 或 CIDR 网段')
    return
  }
  const expires = parseExpires(formExpires.value)
  if (expires !== null && Number.isNaN(expires)) {
    toast.error('拉黑时长必须是数字（秒）')
    return
  }
  if (expires !== null && expires <= 0) {
    toast.error('拉黑时长必须大于 0 秒，不填则为永久拉黑')
    return
  }
  submitting.value = true
  try {
    await securityApi.create({
      ip_or_cidr: ip,
      reason: formReason.value.trim() || null,
      expires_in_seconds: expires,
    })
    toast.success(`已拉黑 ${ip}`)
    creating.value = false
    page.value = 1
    await load()
  } catch (e) {
    toast.error((e as Error).message)
  } finally {
    submitting.value = false
  }
}

// ── 解拉黑 ──

async function confirmRemove(): Promise<void> {
  if (!removing.value) return
  const target = removing.value
  removing.value = null
  try {
    await securityApi.remove(target.id)
    toast.success(`已解除拉黑 ${target.ip_or_cidr}`)
    await load()
  } catch (e) {
    toast.error((e as Error).message)
  }
}

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

  <!-- 拉黑弹窗 -->
  <AppModal :open="creating" title="拉黑 IP / CIDR" @close="creating = false">
    <div class="space-y-4">
      <AppInput
        v-model="formIp"
        label="IP / 网段"
        placeholder="如 1.2.3.4、10.0.0.0/24、2001:db8::/32"
        required
      />
      <AppInput v-model="formReason" label="原因" placeholder="拉黑原因 / 备注" />
      <AppInput
        v-model="formExpires"
        label="时长（秒，可选）"
        type="number"
        placeholder="不填 = 永久拉黑"
      />
      <p class="text-xs text-slate-500">
        拉黑后数据面数秒内生效，对 HTTP / WebSocket / TCP 隧道三协议统一拦截（403）。
        重复拉黑同一 IP / 网段会刷新原因与过期时间。
        <template v-if="parseExpires(formExpires) !== null && !Number.isNaN(parseExpires(formExpires))">
          <br />本次拉黑时长：{{ fmtDuration(parseExpires(formExpires) as number) }}
        </template>
      </p>
    </div>
    <template #footer>
      <AppButton variant="secondary" @click="creating = false">取消</AppButton>
      <AppButton :loading="submitting" @click="submitCreate">确认拉黑</AppButton>
    </template>
  </AppModal>

  <!-- 解拉黑确认 -->
  <AppModal :open="removing !== null" title="解除拉黑" @close="removing = null">
    <p class="text-sm text-slate-600">
      确定解除 <code class="rounded bg-slate-100 px-1.5 py-0.5 text-xs font-medium text-slate-700">{{ removing?.ip_or_cidr }}</code> 的拉黑吗？
      解除后该 IP 立即可重新访问网关。
    </p>
    <template #footer>
      <AppButton variant="secondary" @click="removing = null">取消</AppButton>
      <AppButton variant="danger" @click="confirmRemove">确认解除</AppButton>
    </template>
  </AppModal>
</template>
