<script setup lang="ts">
/**
 * 按钮组件。
 * 用法：
 *   <AppButton variant="danger" size="sm" @click="remove">删除</AppButton>
 *   <AppButton loading>提交中</AppButton>
 */
import { computed } from 'vue'

type ButtonVariant = 'primary' | 'secondary' | 'danger' | 'ghost'
type ButtonSize = 'sm' | 'md' | 'lg'

const props = withDefaults(
  defineProps<{
    variant?: ButtonVariant
    size?: ButtonSize
    /** 禁用态 */
    disabled?: boolean
    /** 加载中（显示转圈并禁用点击） */
    loading?: boolean
    type?: 'button' | 'submit'
  }>(),
  { variant: 'primary', size: 'md', disabled: false, loading: false, type: 'button' },
)

const emit = defineEmits<{ click: [event: MouseEvent] }>()

const classes = computed(() => {
  const base =
    'inline-flex items-center justify-center gap-1.5 rounded-md font-medium transition ' +
    'focus:outline-none focus:ring-2 focus:ring-offset-1 disabled:cursor-not-allowed disabled:opacity-50'
  const variants: Record<ButtonVariant, string> = {
    primary: 'bg-indigo-600 text-white hover:bg-indigo-700 focus:ring-indigo-300',
    secondary: 'bg-white text-slate-700 border border-slate-300 hover:bg-slate-50 focus:ring-slate-200',
    danger: 'bg-red-600 text-white hover:bg-red-700 focus:ring-red-300',
    ghost: 'text-slate-500 hover:bg-slate-100 focus:ring-slate-200',
  }
  const sizes: Record<ButtonSize, string> = {
    sm: 'h-7 px-2.5 text-xs',
    md: 'h-9 px-3.5 text-sm',
    lg: 'h-10 px-5 text-sm',
  }
  return [base, variants[props.variant], sizes[props.size]].join(' ')
})

function onClick(e: MouseEvent): void {
  if (!props.disabled && !props.loading) {
    emit('click', e)
  }
}
</script>

<template>
  <button :type="type" :disabled="disabled || loading" :class="classes" @click="onClick">
    <!-- 加载指示 -->
    <svg v-if="loading" class="h-4 w-4 animate-spin" viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" />
      <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8v4a4 4 0 00-4 4H4z" />
    </svg>
    <slot />
  </button>
</template>
