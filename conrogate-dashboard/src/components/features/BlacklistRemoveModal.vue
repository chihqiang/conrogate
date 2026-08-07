<script setup lang="ts">
/**
 * 解除拉黑确认弹窗。
 *
 * 用法：
 *   <BlacklistRemoveModal :item="removing" @close="removing = null" @removed="reload" />
 */
import { ref } from 'vue'
import { securityApi } from '@/api/security'
import { useToastStore } from '@/stores/toast'
import AppButton from '@/components/ui/AppButton.vue'
import AppModal from '@/components/ui/AppModal.vue'
import type { IpBlacklistDto } from '@/types'

const props = defineProps<{
  /** 待解除条目；null 表示关闭 */
  item: IpBlacklistDto | null
}>()

const emit = defineEmits<{
  close: []
  /** 解除成功（父级可在此刷新列表） */
  removed: []
}>()

const toast = useToastStore()
const loading = ref(false)

async function confirmRemove(): Promise<void> {
  if (!props.item) return
  loading.value = true
  try {
    await securityApi.remove(props.item.id)
    toast.success(`已解除拉黑 ${props.item.ip_or_cidr}`)
    emit('removed')
    emit('close')
  } catch (e) {
    toast.error((e as Error).message)
  } finally {
    loading.value = false
  }
}
</script>

<template>
  <AppModal :open="item !== null" title="解除拉黑" @close="emit('close')">
    <p class="text-sm text-slate-600">
      确定解除
      <code class="rounded bg-slate-100 px-1.5 py-0.5 text-xs font-medium text-slate-700">{{ item?.ip_or_cidr }}</code>
      的拉黑吗？解除后该 IP 立即可重新访问网关。
    </p>
    <template #footer>
      <AppButton variant="secondary" @click="emit('close')">取消</AppButton>
      <AppButton variant="danger" :loading="loading" @click="confirmRemove">确认解除</AppButton>
    </template>
  </AppModal>
</template>
