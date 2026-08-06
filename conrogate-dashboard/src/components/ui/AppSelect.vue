<script setup lang="ts">
/**
 * 下拉选择组件，通过 defineModel 支持 v-model。
 * 用法：
 *   <AppSelect v-model="protocol" :options="protocolOptions" label="协议" />
 */
export interface SelectOption {
  value: string | number
  label: string
}

withDefaults(
  defineProps<{
    label?: string
    placeholder?: string
    options?: SelectOption[]
    disabled?: boolean
  }>(),
  { label: '', placeholder: '请选择', options: () => [], disabled: false },
)

const model = defineModel<string | number | null>()

/** 转发原生 change 事件（供外部监听选项变化） */
const emit = defineEmits<{ change: [] }>()
</script>

<template>
  <label class="block">
    <span v-if="label" class="mb-1 block text-sm font-medium text-slate-700">{{ label }}</span>
    <select
      v-model="model"
      :disabled="disabled"
      class="h-9 w-full rounded-md border border-slate-300 bg-white px-2.5 text-sm outline-none transition focus:border-indigo-500 focus:ring-1 focus:ring-indigo-500 disabled:bg-slate-100"
      @change="emit('change')"
    >
      <!-- 空值占位项 -->
      <option value="" disabled>{{ placeholder }}</option>
      <option v-for="opt in options" :key="opt.value" :value="opt.value">{{ opt.label }}</option>
    </select>
  </label>
</template>
