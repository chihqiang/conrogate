<script setup lang="ts">
/**
 * 全局 Toast 渲染层（挂在根组件 App.vue，无需手动引入）。
 * 数据来自 stores/toast.ts。
 */
import { useToastStore } from '@/stores/toast'

const toast = useToastStore()

/** 按类型返回底色类 */
function toneClass(type: string): string {
  switch (type) {
    case 'success':
      return 'bg-emerald-600'
    case 'error':
      return 'bg-red-600'
    default:
      return 'bg-slate-700'
  }
}
</script>

<template>
  <Teleport to="body">
    <!-- 右上角堆叠区域 -->
    <div class="pointer-events-none fixed right-4 top-4 z-[60] flex w-80 flex-col gap-2">
      <TransitionGroup name="toast">
        <div
          v-for="t in toast.toasts"
          :key="t.id"
          :class="['pointer-events-auto rounded-md px-4 py-3 text-sm text-white shadow-lg', toneClass(t.type)]"
        >
          {{ t.message }}
        </div>
      </TransitionGroup>
    </div>
  </Teleport>
</template>

<style scoped>
.toast-enter-active,
.toast-leave-active {
  transition: all 0.2s ease;
}
.toast-enter-from {
  opacity: 0;
  transform: translateX(1rem);
}
.toast-leave-to {
  opacity: 0;
  transform: translateX(1rem);
}
</style>
