<script setup lang="ts">
/**
 * 版本差异对比弹窗（任意两个版本的新增 / 修改 / 删除）。
 *
 * 用法：
 *   <ConfigDiffModal v-model:open="open" :versions="versions" />
 */
import { computed, ref, watch } from 'vue'
import { configApi } from '@/api/configs'
import { useToastStore } from '@/stores/toast'
import AppModal from '@/components/ui/AppModal.vue'
import AppSelect from '@/components/ui/AppSelect.vue'
import type { ConfigDiff, ConfigVersionDto } from '@/types'

const props = defineProps<{
  open: boolean
  versions: ConfigVersionDto[]
}>()

const emit = defineEmits<{
  'update:open': [value: boolean]
}>()

const toast = useToastStore()

const diffFrom = ref<number | null>(null)
const diffTo = ref<number | null>(null)
const diff = ref<ConfigDiff | null>(null)
const diffLoading = ref(false)

/** 版本列表（用于 diff 下拉） */
const versionOptions = computed(() => props.versions.map((v) => ({ value: v.version, label: `v${v.version}` })))

// 每次打开时按最新 / 最旧版本初始化并拉取差异
watch(
  () => props.open,
  (v) => {
    if (!v) return
    if (props.versions.length < 2) {
      toast.info('至少需要两个历史版本才能对比')
      emit('update:open', false)
      return
    }
    diffFrom.value = props.versions[props.versions.length - 1]?.version ?? null
    diffTo.value = props.versions[0]?.version ?? null
    diff.value = null
    void fetchDiff()
  },
)

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
</script>

<template>
  <AppModal :open="open" title="版本差异对比" width="max-w-2xl" @close="emit('update:open', false)">
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
