<script setup lang="ts">
/**
 * 分页组件（受控组件）。
 * 用法：
 *   <AppPagination :total="total" :page="page" :page-size="pageSize" @update:page="load(page)" />
 */
import { computed } from 'vue'
import AppButton from '@/components/ui/AppButton.vue'

const props = withDefaults(
  defineProps<{
    total: number
    page: number
    pageSize: number
  }>(),
  { total: 0, page: 1, pageSize: 20 },
)

const emit = defineEmits<{ 'update:page': [page: number] }>()

const pageCount = computed(() => Math.max(1, Math.ceil(props.total / props.pageSize)))

function setPage(p: number): void {
  if (p < 1 || p > pageCount.value) return
  emit('update:page', p)
}
</script>

<template>
  <div class="flex items-center justify-between px-1 py-3 text-sm text-slate-500">
    <span>共 {{ total }} 条，第 {{ page }} / {{ pageCount }} 页</span>
    <div class="flex items-center gap-2">
      <AppButton variant="secondary" size="sm" :disabled="page <= 1" @click="setPage(page - 1)">
        上一页
      </AppButton>
      <AppButton
        variant="secondary"
        size="sm"
        :disabled="page >= pageCount"
        @click="setPage(page + 1)"
      >
        下一页
      </AppButton>
    </div>
  </div>
</template>
