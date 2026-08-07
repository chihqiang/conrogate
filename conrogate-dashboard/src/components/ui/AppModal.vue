<script setup lang="ts">
/**
 * 模态框组件（Teleport 到 body，点击遮罩 / 按 Esc 关闭）。
 * 用法：
 *   <AppModal :open="visible" title="新建路由" @close="visible = false">
 *     表单...
 *     <template #footer>
 *       <AppButton variant="secondary" @click="visible=false">取消</AppButton>
 *       <AppButton @click="save">保存</AppButton>
 *     </template>
 *   </AppModal>
 */
import { onBeforeUnmount, watch } from 'vue'

const props = withDefaults(
  defineProps<{
    open: boolean
    title: string
    /** 宽度类（Tailwind max-w-*），默认中宽 */
    width?: string
  }>(),
  { width: 'max-w-lg' },
)

const emit = defineEmits<{ close: [] }>()

function onBackdrop(): void {
  emit('close')
}

function onKeydown(e: KeyboardEvent): void {
  if (e.key === 'Escape') emit('close')
}

// 打开时监听 Esc，关闭/卸载时移除
watch(
  () => props.open,
  (v) => {
    if (v) window.addEventListener('keydown', onKeydown)
    else window.removeEventListener('keydown', onKeydown)
  },
)

onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKeydown)
})
</script>

<template>
  <Teleport to="body">
    <Transition name="fade">
      <div v-if="open" class="fixed inset-0 z-50 flex items-center justify-center p-4">
        <!-- 遮罩层 -->
        <div class="absolute inset-0 bg-slate-900/50" @click="onBackdrop" />
        <div :class="['relative w-full rounded-lg bg-white shadow-xl', width]">
          <!-- 标题栏 -->
          <div class="flex items-center justify-between border-b border-slate-200 px-5 py-3">
            <h3 class="text-base font-semibold text-slate-800">{{ title }}</h3>
            <button
              class="rounded p-1 text-slate-400 transition hover:bg-slate-100 hover:text-slate-600"
              aria-label="关闭"
              @click="emit('close')"
            >
              <svg class="h-4 w-4" viewBox="0 0 20 20" fill="currentColor" aria-hidden="true">
                <path d="M6.28 5.22a.75.75 0 00-1.06 1.06L8.94 10l-3.72 3.72a.75.75 0 101.06 1.06L10 11.06l3.72 3.72a.75.75 0 101.06-1.06L11.06 10l3.72-3.72a.75.75 0 00-1.06-1.06L10 8.94 6.28 5.22z" />
              </svg>
            </button>
          </div>
          <!-- 内容区 -->
          <div class="px-5 py-4">
            <slot />
          </div>
          <!-- 底部操作区（可选） -->
          <div v-if="$slots.footer" class="flex justify-end gap-2 border-t border-slate-200 px-5 py-3">
            <slot name="footer" />
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.15s ease;
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>
