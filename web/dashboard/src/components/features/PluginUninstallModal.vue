<script setup lang="ts">
/**
 * 卸载插件确认弹窗。
 *
 * 用法：
 *   <PluginUninstallModal :plugin="uninstalling" @close="uninstalling = null" @uninstalled="reload" />
 */
import { ref } from 'vue'
import { pluginApi } from '@/api/plugins'
import { useToastStore } from '@/stores/toast'
import AppButton from '@/components/ui/AppButton.vue'
import AppModal from '@/components/ui/AppModal.vue'
import type { InstalledPluginDto } from '@/types'

const props = defineProps<{
  /** 待卸载插件；null 表示关闭 */
  plugin: InstalledPluginDto | null
}>()

const emit = defineEmits<{
  close: []
  /** 卸载成功（父级可在此刷新列表） */
  uninstalled: []
}>()

const toast = useToastStore()
const loading = ref(false)

async function confirmUninstall(): Promise<void> {
  if (!props.plugin) return
  const name = props.plugin.name
  loading.value = true
  try {
    await pluginApi.uninstall(name)
    toast.success(`已卸载插件「${name}」`)
    emit('uninstalled')
    emit('close')
  } catch (e) {
    toast.error((e as Error).message)
  } finally {
    loading.value = false
  }
}
</script>

<template>
  <AppModal :open="plugin !== null" title="卸载插件" @close="emit('close')">
    <p class="text-sm text-slate-600">
      确定卸载插件 <span class="font-medium text-slate-800">{{ plugin?.name }}</span> 吗？
      卸载后该插件将无法继续绑定到路由。
    </p>
    <template #footer>
      <AppButton variant="secondary" @click="emit('close')">取消</AppButton>
      <AppButton variant="danger" :loading="loading" @click="confirmUninstall">卸载</AppButton>
    </template>
  </AppModal>
</template>
