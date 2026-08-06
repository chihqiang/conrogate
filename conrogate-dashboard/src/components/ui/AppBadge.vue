<script setup lang="ts">
/**
 * 状态徽标组件。
 * 用法：<AppBadge tone="green">已启用</AppBadge>
 */
import { computed } from 'vue'

type Tone = 'green' | 'red' | 'yellow' | 'blue' | 'gray' | 'indigo'

const props = withDefaults(defineProps<{ tone?: Tone }>(), { tone: 'gray' })

const toneMap: Record<Tone, string> = {
  green: 'bg-emerald-50 text-emerald-700 ring-emerald-200',
  red: 'bg-red-50 text-red-700 ring-red-200',
  yellow: 'bg-amber-50 text-amber-700 ring-amber-200',
  blue: 'bg-sky-50 text-sky-700 ring-sky-200',
  gray: 'bg-slate-100 text-slate-600 ring-slate-200',
  indigo: 'bg-indigo-50 text-indigo-700 ring-indigo-200',
}

const classes = computed(() => toneMap[props.tone])
</script>

<template>
  <span
    :class="[
      'inline-flex items-center rounded-full px-2 py-0.5 text-xs font-medium ring-1 ring-inset',
      classes,
    ]"
  >
    <slot />
  </span>
</template>
