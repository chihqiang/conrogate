<script setup lang="ts">
/**
 * 回滚配置确认弹窗。
 *
 * 用法：
 *   <ConfigRollbackModal :version="rollbackTarget" @close="rollbackTarget = null" @rolledback="reload" />
 */
import { ref } from 'vue'
import { configApi } from '@/api/configs'
import { useToastStore } from '@/stores/toast'
import AppButton from '@/components/ui/AppButton.vue'
import AppModal from '@/components/ui/AppModal.vue'
import type { ConfigVersionDto } from '@/types'

const props = defineProps<{
  /** 回滚目标版本；null 表示关闭 */
  version: ConfigVersionDto | null
}>()

const emit = defineEmits<{
  close: []
  /** 回滚成功（父级可在此刷新版本列表） */
  rolledback: []
}>()

const toast = useToastStore()
const loading = ref(false)

async function rollback(): Promise<void> {
  if (!props.version) return
  loading.value = true
  try {
    await configApi.rollback(props.version.version)
    toast.success(`已回滚到 v${props.version.version}`)
    emit('rolledback')
    emit('close')
  } catch (e) {
    toast.error((e as Error).message)
  } finally {
    loading.value = false
  }
}
</script>

<template>
  <AppModal :open="version !== null" title="回滚配置" @close="emit('close')">
    <p class="text-sm text-slate-600">
      确定回滚到版本 <span class="font-medium text-slate-800">v{{ version?.version }}</span> 吗？
      系统将基于该版本快照生成一个新的回滚版本并热载。
    </p>
    <template #footer>
      <AppButton variant="secondary" @click="emit('close')">取消</AppButton>
      <AppButton :loading="loading" @click="rollback">回滚</AppButton>
    </template>
  </AppModal>
</template>
