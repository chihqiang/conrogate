<script setup lang="ts">
/**
 * 解绑插件确认弹窗。
 *
 * 用法：
 *   <RouteUnbindModal :route-id="routeId" :binding="unbinding" @close="unbinding = null" @unbound="reload" />
 */
import { ref } from 'vue'
import { pluginApi } from '@/api/plugins'
import { useToastStore } from '@/stores/toast'
import AppButton from '@/components/ui/AppButton.vue'
import AppModal from '@/components/ui/AppModal.vue'
import type { PluginBindingDto } from '@/types'

const props = defineProps<{
  routeId: number
  /** 待解绑的绑定记录；null 表示关闭 */
  binding: PluginBindingDto | null
}>()

const emit = defineEmits<{
  close: []
  /** 解绑成功（父级可在此刷新绑定列表） */
  unbound: []
}>()

const toast = useToastStore()
const loading = ref(false)

async function confirmUnbind(): Promise<void> {
  if (!props.binding) return
  loading.value = true
  try {
    await pluginApi.unbind(props.routeId, props.binding.plugin_name)
    toast.success(`已解绑插件「${props.binding.plugin_name}」`)
    emit('unbound')
    emit('close')
  } catch (e) {
    toast.error((e as Error).message)
  } finally {
    loading.value = false
  }
}
</script>

<template>
  <AppModal :open="binding !== null" title="解绑插件" @close="emit('close')">
    <p class="text-sm text-slate-600">
      确定从该路由解绑插件 <span class="font-medium text-slate-800">{{ binding?.plugin_name }}</span> 吗？
    </p>
    <template #footer>
      <AppButton variant="secondary" @click="emit('close')">取消</AppButton>
      <AppButton variant="danger" :loading="loading" @click="confirmUnbind">解绑</AppButton>
    </template>
  </AppModal>
</template>
