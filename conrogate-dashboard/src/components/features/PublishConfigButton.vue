<script setup lang="ts">
/**
 * 发布配置按钮 + 弹窗（自包含：按钮触发弹窗，无需父级维护开关状态）。
 * 任意管理页可直接放置；发布成功后通过 published 事件通知父级刷新。
 *
 * 用法：
 *   <PublishConfigButton @published="reload" />
 */
import { ref } from 'vue'
import { useAuthStore } from '@/stores/auth'
import AppButton from '@/components/ui/AppButton.vue'
import ConfigPublishModal from '@/components/features/ConfigPublishModal.vue'

withDefaults(
  defineProps<{
    size?: 'sm' | 'md' | 'lg'
  }>(),
  { size: 'sm' },
)

const emit = defineEmits<{ published: [] }>()

const auth = useAuthStore()
const open = ref(false)
</script>

<template>
  <AppButton v-if="auth.canWrite" :size="size" @click="open = true">发布配置</AppButton>
  <ConfigPublishModal v-model:open="open" @published="emit('published')" />
</template>
