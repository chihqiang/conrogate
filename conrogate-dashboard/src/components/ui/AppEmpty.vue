<script setup lang="ts">
/**
 * 空态占位组件。
 * 用法：<AppEmpty text="暂无路由" />
 */
withDefaults(defineProps<{ text?: string }>(), { text: '暂无数据' })
</script>

<template>
  <div class="flex flex-col items-center justify-center gap-3 py-12 text-slate-400">
    <svg class="h-12 w-12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.2" aria-hidden="true">
      <path d="M20 13V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2h7" stroke-linecap="round" />
      <path d="M15 19l2 2 4-4" stroke-linecap="round" stroke-linejoin="round" />
    </svg>
    <p class="text-sm">{{ text }}</p>
    <slot />
  </div>
</template>
