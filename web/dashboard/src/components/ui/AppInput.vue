<script setup lang="ts">
/**
 * 文本输入组件，通过 defineModel 支持 v-model。
 * 用法：
 *   <AppInput v-model="name" label="路由名称" placeholder="例如：product-api" />
 */
withDefaults(
  defineProps<{
    label?: string
    placeholder?: string
    type?: string
    disabled?: boolean
    /** 是否必填（label 后显示红色 *） */
    required?: boolean
    /** 输入提示（help text，在输入框下方显示） */
    hint?: string
  }>(),
  { label: '', placeholder: '', type: 'text', disabled: false, required: false, hint: '' },
)

/** v-model 值 */
const model = defineModel<string | number>()
</script>

<template>
  <label class="block">
    <span v-if="label" class="mb-1.5 block text-sm font-medium text-slate-700">
      {{ label }}
      <span v-if="required" class="text-red-500">*</span>
    </span>
    <input
      v-model="model"
      :type="type"
      :placeholder="placeholder"
      :disabled="disabled"
      class="h-9.5 w-full rounded-lg border border-slate-300 bg-white px-3 text-sm outline-none transition-all duration-150 placeholder:text-slate-400 focus:border-indigo-500 focus:ring-2 focus:ring-indigo-500/20 hover:border-slate-400 disabled:bg-slate-100"
    />
    <span v-if="hint" class="mt-1 block text-xs text-slate-400">{{ hint }}</span>
  </label>
</template>
