<script setup lang="ts">
/**
 * 删除上游确认弹窗。
 * 被路由绑定的上游禁止删除：打开时查询绑定路由并展示，删除按钮置灰。
 *
 * 用法：
 *   <UpstreamDeleteModal :upstream="deleting" @close="deleting = null" @deleted="onDeleted" />
 */
import { ref, watch } from 'vue'
import { upstreamApi } from '@/api/upstreams'
import { useToastStore } from '@/stores/toast'
import AppButton from '@/components/ui/AppButton.vue'
import AppModal from '@/components/ui/AppModal.vue'
import type { UpstreamDto, UpstreamRouteBindingDto } from '@/types'

const props = defineProps<{
  /** 待删除上游；null 表示关闭 */
  upstream: UpstreamDto | null
}>()

const emit = defineEmits<{
  close: []
  /** 删除成功（父级可在此处理翻页回退并刷新列表） */
  deleted: []
}>()

const toast = useToastStore()
const loading = ref(false)

/** 绑定该上游的活跃路由 */
const bindings = ref<UpstreamRouteBindingDto[]>([])
const checking = ref(false)

/** 是否被路由绑定（决定是否允许删除） */
const hasBindings = ref(false)

// 每次打开时查询绑定关系
watch(
  () => props.upstream,
  async (upstream) => {
    bindings.value = []
    hasBindings.value = false
    if (!upstream) return
    checking.value = true
    try {
      bindings.value = await upstreamApi.routeBindings(upstream.id)
      hasBindings.value = bindings.value.length > 0
    } catch (e) {
      // 查询失败不阻塞删除，交由后端删除接口校验兜底
      toast.error((e as Error).message)
    } finally {
      checking.value = false
    }
  },
)

async function confirmDelete(): Promise<void> {
  if (!props.upstream || hasBindings.value) return
  loading.value = true
  try {
    await upstreamApi.remove(props.upstream.id)
    toast.success(`已删除上游「${props.upstream.name}」`)
    emit('deleted')
    emit('close')
  } catch (e) {
    toast.error((e as Error).message)
  } finally {
    loading.value = false
  }
}
</script>

<template>
  <AppModal :open="upstream !== null" title="删除上游" @close="emit('close')">
    <!-- 被绑定：禁止删除 -->
    <div v-if="hasBindings" class="space-y-3">
      <div class="rounded-md border border-amber-200 bg-amber-50 p-3 text-sm text-amber-800">
        <p class="font-medium">该上游正被 {{ bindings.length }} 个路由绑定，无法删除。</p>
        <p class="mt-1 text-xs text-amber-700">
          请先在「路由管理」中解除这些路由的上游绑定，再回来删除。
        </p>
      </div>
      <div>
        <p class="mb-1.5 text-xs font-medium text-slate-500">绑定路由：</p>
        <ul class="space-y-1">
          <li
            v-for="b in bindings"
            :key="b.id"
            class="rounded bg-slate-100 px-2 py-1 font-mono text-xs text-slate-700"
          >
            route#{{ b.id }} · {{ b.name }}
          </li>
        </ul>
      </div>
    </div>

    <!-- 未绑定：正常确认 -->
    <p v-else class="text-sm text-slate-600">
      确定删除上游 <span class="font-medium text-slate-800">{{ upstream?.name }}</span> 吗？其下节点将一并失效。
    </p>

    <template #footer>
      <AppButton variant="secondary" @click="emit('close')">取消</AppButton>
      <AppButton
        variant="danger"
        :loading="loading || checking"
        :disabled="hasBindings"
        @click="confirmDelete"
      >
        {{ hasBindings ? '无法删除' : '删除' }}
      </AppButton>
    </template>
  </AppModal>
</template>
