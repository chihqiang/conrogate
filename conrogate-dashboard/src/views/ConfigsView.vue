<script setup lang="ts">
/**
 * 配置版本页：版本历史 + 发布新版本 + 回滚 + 版本差异对比。
 * 对应控制面接口见 docs/api.md「6. 配置版本管理」。
 */
import { computed, onMounted, ref } from 'vue'
import { configApi } from '@/api/configs'
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
import { PublishType, PublishTypeLabels } from '@/types/enums'
import type { ConfigDiff, ConfigVersionDto } from '@/types'

// ── 状态 ──

const auth = useAuthStore()
const toast = useToastStore()

const versions = ref<ConfigVersionDto[]>([])
const total = ref(0)
const page = ref(1)
const pageSize = 20
const loading = ref(false)

// 发布
const publishOpen = ref(false)
const publishRemark = ref('')
const publishing = ref(false)

// 回滚
const rollbackTarget = ref<ConfigVersionDto | null>(null)
const rollingBack = ref(false)

// 差异对比
const diffOpen = ref(false)
const diffFrom = ref<number | null>(null)
const diffTo = ref<number | null>(null)
const diff = ref<ConfigDiff | null>(null)
const diffLoading = ref(false)

/** 版本列表（用于 diff 下拉） */
const versionOptions = computed(() => versions.value.map((v) => ({ value: v.version, label: `v${v.version}` })))

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

// ── 发布 ──

function openPublish(): void {
  publishRemark.value = ''
  publishOpen.value = true
}

async function publish(): Promise<void> {
  publishing.value = true
  try {
    // 基准版本取当前最新版本号；无历史版本时为 0（全量快照）
    const latest = versions.value[0]
    await configApi.publish({ base_version: latest ? latest.version : 0, remark: publishRemark.value.trim() || undefined })
    toast.success('配置已发布')
    publishOpen.value = false
    await load()
  } catch (e) {
    toast.error((e as Error).message)
  } finally {
    publishing.value = false
  }
}

// ── 回滚 ──

async function rollback(): Promise<void> {
  if (!rollbackTarget.value) return
  rollingBack.value = true
  try {
    await configApi.rollback(rollbackTarget.value.version)
    toast.success(`已回滚到 v${rollbackTarget.value.version}`)
    rollbackTarget.value = null
    await load()
  } catch (e) {
    toast.error((e as Error).message)
  } finally {
    rollingBack.value = false
  }
}

// ── 差异对比 ──

async function openDiff(): Promise<void> {
  if (versions.value.length < 2) {
    toast.info('至少需要两个历史版本才能对比')
    return
  }
  diffFrom.value = versions.value[versions.value.length - 1]?.version ?? null
  diffTo.value = versions.value[0]?.version ?? null
  diff.value = null
  diffOpen.value = true
  await fetchDiff()
}

async function fetchDiff(): Promise<void> {
  if (diffFrom.value === null || diffTo.value === null || diffFrom.value === diffTo.value) {
    diff.value = null
    return
  }
  diffLoading.value = true
  try {
    diff.value = await configApi.diff(diffFrom.value, diffTo.value)
  } catch (e) {
    toast.error((e as Error).message)
  } finally {
    diffLoading.value = false
  }
}

// ── 挂载 ──

onMounted(() => void load())
</script>

<template>
  <AppCard title="配置版本历史">
    <template #actions>
      <AppButton v-if="auth.canWrite" variant="secondary" size="sm" @click="openDiff">版本对比</AppButton>
      <AppButton v-if="auth.canWrite" size="sm" @click="openPublish">发布配置</AppButton>
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
          <AppButton v-if="auth.canWrite" variant="ghost" size="sm" @click="openDiff">对比</AppButton>
        </div>
      </template>

      <template #empty>
        <AppEmpty text="暂无版本记录，点击右上角「发布配置」生成首个版本" />
      </template>
    </AppTable>

    <AppPagination :total="total" :page="page" :page-size="pageSize" @update:page="page = $event; load()" />
  </AppCard>

  <!-- 发布弹窗 -->
  <AppModal :open="publishOpen" title="发布配置" @close="publishOpen = false">
    <p class="mb-4 text-sm text-slate-500">
      将当前全部配置（路由 + 上游 + 插件绑定）快照为新版本。发布后数据面将在数秒内热载生效。
    </p>
    <AppInput v-model="publishRemark" label="备注" placeholder="例如 release v2.1（可留空）" />
    <template #footer>
      <AppButton variant="secondary" @click="publishOpen = false">取消</AppButton>
      <AppButton :loading="publishing" @click="publish">发布</AppButton>
    </template>
  </AppModal>

  <!-- 回滚确认 -->
  <AppModal :open="rollbackTarget !== null" title="回滚配置" @close="rollbackTarget = null">
    <p class="text-sm text-slate-600">
      确定回滚到版本 <span class="font-medium text-slate-800">v{{ rollbackTarget?.version }}</span> 吗？
      系统将基于该版本快照生成一个新的回滚版本并热载。
    </p>
    <template #footer>
      <AppButton variant="secondary" @click="rollbackTarget = null">取消</AppButton>
      <AppButton :loading="rollingBack" @click="rollback">回滚</AppButton>
    </template>
  </AppModal>

  <!-- 版本对比弹窗 -->
  <AppModal :open="diffOpen" title="版本差异对比" :width="'max-w-2xl'" @close="diffOpen = false">
    <div class="mb-4 grid grid-cols-2 gap-4">
      <AppSelect
        v-model="diffFrom"
        label="源版本（from）"
        :options="versionOptions"
        @change="fetchDiff"
      />
      <AppSelect
        v-model="diffTo"
        label="目标版本（to）"
        :options="versionOptions"
        @change="fetchDiff"
      />
    </div>

    <div v-if="diffLoading" class="py-8 text-center text-sm text-slate-400">加载差异...</div>
    <div v-else-if="!diff" class="py-8 text-center text-sm text-slate-400">请选择两个不同版本进行对比</div>
    <div v-else class="space-y-4">
      <div>
        <h4 class="mb-1 text-xs font-semibold text-emerald-600">新增（{{ diff.added.length }}）</h4>
        <ul class="list-inside list-disc space-y-0.5 text-sm text-slate-600">
          <li v-for="item in diff.added" :key="item">{{ item }}</li>
          <li v-if="diff.added.length === 0" class="list-none text-slate-400">无</li>
        </ul>
      </div>
      <div>
        <h4 class="mb-1 text-xs font-semibold text-amber-600">修改（{{ diff.modified.length }}）</h4>
        <ul class="list-inside list-disc space-y-0.5 text-sm text-slate-600">
          <li v-for="item in diff.modified" :key="item">{{ item }}</li>
          <li v-if="diff.modified.length === 0" class="list-none text-slate-400">无</li>
        </ul>
      </div>
      <div>
        <h4 class="mb-1 text-xs font-semibold text-red-600">删除（{{ diff.removed.length }}）</h4>
        <ul class="list-inside list-disc space-y-0.5 text-sm text-slate-600">
          <li v-for="item in diff.removed" :key="item">{{ item }}</li>
          <li v-if="diff.removed.length === 0" class="list-none text-slate-400">无</li>
        </ul>
      </div>
    </div>
  </AppModal>
</template>
