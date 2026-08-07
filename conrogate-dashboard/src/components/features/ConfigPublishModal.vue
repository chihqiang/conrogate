<script setup lang="ts">
/**
 * 发布配置弹窗（全局操作：将路由 + 上游 + 插件绑定快照为新版本）。
 * 可被任意管理页复用；发布成功通过 published 事件通知父级刷新。
 *
 * 用法：
 *   <ConfigPublishModal v-model:open="open" @published="reload" />
 */
import { ref, watch } from 'vue'
import { configApi } from '@/api/configs'
import { useToastStore } from '@/stores/toast'
import AppButton from '@/components/ui/AppButton.vue'
import AppInput from '@/components/ui/AppInput.vue'
import AppModal from '@/components/ui/AppModal.vue'

const props = defineProps<{
  /** 弹窗是否可见（v-model:open） */
  open: boolean
}>()

const emit = defineEmits<{
  'update:open': [value: boolean]
  /** 发布成功（父级可在此刷新列表） */
  published: []
}>()

const toast = useToastStore()

const remark = ref('')
const publishing = ref(false)

// 每次打开时清空备注
watch(
  () => props.open,
  (v) => {
    if (v) remark.value = ''
  },
)

function close(): void {
  if (!publishing.value) emit('update:open', false)
}

async function publish(): Promise<void> {
  publishing.value = true
  try {
    // 基准版本取当前最新版本号；无历史版本时为 0（全量快照）
    const res = await configApi.versions({ page: 1, page_size: 1 })
    const latest = res.list[0]
    await configApi.publish({ base_version: latest ? latest.version : 0, remark: remark.value.trim() || undefined })
    toast.success('配置已发布')
    emit('published')
    emit('update:open', false)
  } catch (e) {
    toast.error((e as Error).message)
  } finally {
    publishing.value = false
  }
}
</script>

<template>
  <AppModal :open="open" title="发布配置" @close="close">
    <p class="mb-4 text-sm text-slate-500">
      将当前全部配置（路由 + 上游 + 插件绑定）快照为新版本。发布后数据面将在数秒内热载生效。
    </p>
    <AppInput v-model="remark" label="备注" placeholder="例如 release v2.1（可留空）" />
    <template #footer>
      <AppButton variant="secondary" @click="close">取消</AppButton>
      <AppButton :loading="publishing" @click="publish">发布</AppButton>
    </template>
  </AppModal>
</template>
