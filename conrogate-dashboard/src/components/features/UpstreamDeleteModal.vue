<script setup lang="ts">
/**
 * 删除上游确认弹窗。
 *
 * 用法：
 *   <UpstreamDeleteModal :upstream="deleting" @close="deleting = null" @deleted="onDeleted" />
 */
import { ref } from 'vue'
import { upstreamApi } from '@/api/upstreams'
import { useToastStore } from '@/stores/toast'
import AppButton from '@/components/ui/AppButton.vue'
import AppModal from '@/components/ui/AppModal.vue'
import type { UpstreamDto } from '@/types'

const props = defineProps<{
  /** 待删除上游；null 表示关闭 */
  upstream: UpstreamDto | null
}>()

const emit = defineEmits<{
  close: []
  /** 删除成功（父级可在此处理翻页回退并刷新列表） */
  deleted: []
}>()

const toast = useToastStore()
const loading = ref(false)

async function confirmDelete(): Promise<void> {
  if (!props.upstream) return
  loading.value = true
  try {
    await upstreamApi.remove(props.upstream.id)
    toast.success(`已删除上游「${props.upstream.name}」`)
    emit('deleted')
    emit('close')
  } catch (e) {
    toast.error((e as Error).message)
  } finally {
    loading.value = false
  }
}
</script>

<template>
  <AppModal :open="upstream !== null" title="删除上游" @close="emit('close')">
    <p class="text-sm text-slate-600">
      确定删除上游 <span class="font-medium text-slate-800">{{ upstream?.name }}</span> 吗？其下节点将一并失效。
    </p>
    <template #footer>
      <AppButton variant="secondary" @click="emit('close')">取消</AppButton>
      <AppButton variant="danger" :loading="loading" @click="confirmDelete">删除</AppButton>
    </template>
  </AppModal>
</template>
