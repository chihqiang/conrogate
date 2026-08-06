<script setup lang="ts">
/**
 * 通用表格组件。
 * 用法：
 *   <AppTable :columns="columns" :rows="routes" :loading="loading">
 *     <template #cell-name="{ row }">... 自定义单元格 ...</template>
 *     <template #empty>暂无路由</template>
 *   </AppTable>
 *
 * 说明：
 * - rows 类型为 unknown[]（泛型组件在 Vue SFC 中受限），需在插槽内做类型断言
 * - 每列可通过 formatter 输出文本；需要复杂内容时用具名插槽 `#cell-{key}`
 */
import { computed } from 'vue'

export interface TableColumn {
  /** 字段 key，同时作为动态插槽名 `cell-{key}` */
  key: string
  label: string
  width?: string
  align?: 'left' | 'center' | 'right'
  /** 单元格文本格式化函数；返回 string 时优先显示（否则取原值） */
  formatter?: (value: unknown, row: unknown) => string
}

const props = withDefaults(
  defineProps<{
    columns: TableColumn[]
    rows: unknown[]
    loading?: boolean
  }>(),
  { loading: false },
)

const hasRows = computed(() => props.rows.length > 0)

/** 读取单元格值：泛型数据统一转 Record 后按键取值 */
function cellValue(row: unknown, key: string): unknown {
  return (row as Record<string, unknown>)[key]
}

/** 单元格展示：formatter 优先，否则原值 */
function cellDisplay(row: unknown, col: TableColumn): unknown {
  const value = cellValue(row, col.key)
  return col.formatter ? col.formatter(value, row) : value
}

/** 列对齐样式 */
function alignClass(col: TableColumn): string {
  const map = { left: 'text-left', center: 'text-center', right: 'text-right' } as const
  return map[col.align ?? 'left']
}
</script>

<template>
  <div class="overflow-x-auto">
    <table class="min-w-full divide-y divide-slate-200 text-sm">
      <thead class="bg-slate-50">
        <tr>
          <th
            v-for="col in columns"
            :key="col.key"
            :style="col.width ? { width: col.width } : undefined"
            :class="['px-4 py-2.5 text-xs font-semibold uppercase tracking-wide text-slate-500', alignClass(col)]"
          >
            {{ col.label }}
          </th>
        </tr>
      </thead>
      <tbody v-if="hasRows" class="divide-y divide-slate-100 bg-white">
        <tr v-for="(row, index) in rows" :key="index" class="transition hover:bg-slate-50">
          <td
            v-for="col in columns"
            :key="col.key"
            :class="['px-4 py-3 text-slate-700', alignClass(col)]"
          >
            <!-- 优先自定义单元格插槽，否则用 formatter/原值 -->
            <slot v-if="$slots[`cell-${col.key}`]" :name="`cell-${col.key}`" :row="row" :value="cellValue(row, col.key)">
              {{ cellDisplay(row, col) }}
            </slot>
            <template v-else>{{ cellDisplay(row, col) }}</template>
          </td>
        </tr>
      </tbody>
    </table>

    <!-- 加载 / 空态 -->
    <div
      v-if="loading"
      class="border-t border-slate-100 py-8 text-center text-sm text-slate-400"
    >
      加载中...
    </div>
    <div
      v-else-if="!hasRows"
      class="border-t border-slate-100 py-8 text-center text-sm text-slate-400"
    >
      <slot name="empty">暂无数据</slot>
    </div>
  </div>
</template>
