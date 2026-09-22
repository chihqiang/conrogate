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
    /** 是否允许选择"无"（null），用于可空的关联字段 */
    allowClear?: boolean
  }>(),
  { label: '', placeholder: '请选择', options: () => [], disabled: false, allowClear: false },
)

const model = defineModel<string | number | null>()

/** 转发原生 change 事件（供外部监听选项变化） */
const emit = defineEmits<{ change: [] }>()
</script>

<template>
  <label class="block">
    <span v-if="label" class="mb-1.5 block text-sm font-medium text-slate-700">{{ label }}</span>
    <select
      v-model="model"
      :disabled="disabled"
      class="h-9.5 w-full cursor-pointer rounded-lg border border-slate-300 bg-white px-2.5 text-sm outline-none transition-all duration-150 focus:border-indigo-500 focus:ring-2 focus:ring-indigo-500/20 hover:border-slate-400 disabled:bg-slate-100"
      @change="emit('change')"
    >
      <!-- 空值占位项 -->
      <option :value="null" disabled>{{ placeholder }}</option>
      <!-- 可清除选项（allowClear 时允许选"无"） -->
      <option v-if="allowClear" :value="null">无</option>
      <option v-for="opt in options" :key="opt.value" :value="opt.value">{{ opt.label }}</option>
    </select>
  </label>
</template>
