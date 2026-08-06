<script setup lang="ts">
/**
 * 卡片容器组件。
 * 用法：
 *   <AppCard title="路由列表">
 *     <template #actions><AppButton size="sm">新建</AppButton></template>
 *     正文...
 *   </AppCard>
 */
withDefaults(defineProps<{ title?: string }>(), { title: '' })
</script>

<template>
  <section class="rounded-lg border border-slate-200 bg-white shadow-sm">
    <!-- 标题栏（有标题或动作插槽时显示） -->
    <header
      v-if="title || $slots.actions"
      class="flex items-center justify-between border-b border-slate-200 px-4 py-3"
    >
      <h2 class="text-sm font-semibold text-slate-800">{{ title }}</h2>
      <div class="flex items-center gap-2">
        <slot name="actions" />
      </div>
    </header>
    <div class="p-4">
      <slot />
    </div>
  </section>
</template>
