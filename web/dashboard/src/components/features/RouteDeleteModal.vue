<script setup lang="ts">
/**
 * 删除路由确认弹窗。
 *
 * 用法：
 *   <RouteDeleteModal :route="deleting" @close="deleting = null" @deleted="onDeleted" />
 */
import { ref } from 'vue'
import { routeApi } from '@/api/routes'
import { useToastStore } from '@/stores/toast'
import AppButton from '@/components/ui/AppButton.vue'
import AppModal from '@/components/ui/AppModal.vue'
import type { RouteDto } from '@/types'

const props = defineProps<{
  /** 待删除路由；null 表示关闭 */
  route: RouteDto | null
}>()

const emit = defineEmits<{
  close: []
  /** 删除成功（父级可在此处理翻页回退并刷新列表） */
  deleted: []
}>()

const toast = useToastStore()
const loading = ref(false)

async function confirmDelete(): Promise<void> {
  if (!props.route) return
  loading.value = true
  try {
    await routeApi.remove(props.route.id)
    toast.success(`已删除路由「${props.route.name}」`)
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
  <AppModal :open="route !== null" title="删除路由" @close="emit('close')">
    <p class="text-sm text-slate-600">
      确定删除路由 <span class="font-medium text-slate-800">{{ route?.name }}</span> 吗？该操作不可恢复。
    </p>
    <template #footer>
      <AppButton variant="secondary" @click="emit('close')">取消</AppButton>
      <AppButton variant="danger" :loading="loading" @click="confirmDelete">删除</AppButton>
    </template>
  </AppModal>
</template>
