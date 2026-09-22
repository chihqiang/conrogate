<script setup lang="ts">
/**
 * 新建 / 编辑上游弹窗（含后端节点编辑器）。
 * upstream 为 null 表示新建，否则为编辑目标。
 *
 * 用法：
 *   <UpstreamFormModal v-model:open="open" :upstream="editing" @saved="reload" />
 */
import { ref, watch } from 'vue'
import { upstreamApi } from '@/api/upstreams'
import { useToastStore } from '@/stores/toast'
import AppButton from '@/components/ui/AppButton.vue'
import AppInput from '@/components/ui/AppInput.vue'
import AppModal from '@/components/ui/AppModal.vue'
import AppSelect from '@/components/ui/AppSelect.vue'
import { BalancerAlgorithm, BalancerAlgorithmLabels, toOptions } from '@/types/enums'
import type { CreateUpstreamNodePayload, CreateUpstreamPayload, UpdateUpstreamPayload, UpstreamDto } from '@/types'

const props = defineProps<{
  open: boolean
  /** null 表示新建，否则编辑该上游 */
  upstream: UpstreamDto | null
}>()

const emit = defineEmits<{
  'update:open': [value: boolean]
  /** 保存成功（父级可在此刷新列表） */
  saved: []
}>()

const toast = useToastStore()

interface NodeForm {
  address: string
  weight: number
  enabled: boolean
}

interface UpstreamForm {
  name: string
  algorithm: BalancerAlgorithm
  retryEnabled: boolean
  nodes: NodeForm[]
}

function emptyForm(): UpstreamForm {
  return {
    name: '',
    algorithm: BalancerAlgorithm.RoundRobin,
    retryEnabled: false,
    nodes: [{ address: '', weight: 1, enabled: true }],
  }
}

function formFromUpstream(row: UpstreamDto): UpstreamForm {
  return {
    name: row.name,
    algorithm: row.algorithm,
    retryEnabled: row.retry_enabled,
    nodes: row.nodes.map((n) => ({ address: n.address, weight: n.weight, enabled: n.enabled })),
  }
}

const form = ref<UpstreamForm>(emptyForm())
const saving = ref(false)

// 每次打开时按编辑目标初始化表单
watch(
  () => props.open,
  (v) => {
    if (v) form.value = props.upstream ? formFromUpstream(props.upstream) : emptyForm()
  },
)

function addNode(): void {
  form.value.nodes.push({ address: '', weight: 1, enabled: true })
}

function removeNode(index: number): void {
  form.value.nodes.splice(index, 1)
}

async function save(): Promise<void> {
  if (!form.value.name.trim()) {
    toast.error('请填写上游名称')
    return
  }
  const validNodes = form.value.nodes.filter((n) => n.address.trim())
  if (validNodes.length === 0) {
    toast.error('至少需要一个后端节点地址')
    return
  }
  const nodes: CreateUpstreamNodePayload[] = validNodes.map((n) => ({
    address: n.address.trim(),
    weight: n.weight,
    enabled: n.enabled,
  }))

  saving.value = true
  try {
    if (props.upstream === null) {
      const payload: CreateUpstreamPayload = {
        name: form.value.name,
        algorithm: form.value.algorithm,
        retry_enabled: form.value.retryEnabled,
        nodes,
      }
      await upstreamApi.create(payload)
      toast.success('上游创建成功')
    } else {
      const payload: UpdateUpstreamPayload = {
        id: props.upstream.id,
        name: form.value.name,
        algorithm: form.value.algorithm,
        retry_enabled: form.value.retryEnabled,
        nodes,
      }
      await upstreamApi.update(payload)
      toast.success('上游已更新')
    }
    emit('saved')
    emit('update:open', false)
  } catch (e) {
    toast.error((e as Error).message)
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <AppModal
    :open="open"
    :title="upstream === null ? '新建上游' : `编辑上游 #${upstream.id}`"
    width="max-w-2xl"
    @close="emit('update:open', false)"
  >
    <form class="space-y-5" @submit.prevent="save">
      <!-- 分区：基本信息 -->
      <div>
        <h4 class="mb-3 text-xs font-semibold uppercase tracking-wide text-slate-400">基本信息</h4>
        <div class="grid grid-cols-3 gap-4">
          <AppInput v-model="form.name" label="上游名称" required placeholder="例如 product-api" />
          <AppSelect v-model="form.algorithm" label="负载均衡算法" :options="toOptions(BalancerAlgorithmLabels)" />
          <label
            class="flex cursor-pointer items-center gap-2 rounded-lg border border-slate-300 p-2.5 text-sm transition-all duration-150 hover:border-slate-400 hover:bg-slate-50"
            :class="form.retryEnabled ? 'border-indigo-500 bg-indigo-50' : ''"
            style="margin-top: 1.375rem"
          >
            <input v-model="form.retryEnabled" type="checkbox" class="h-4 w-4 rounded accent-indigo-600" />
            <span :class="form.retryEnabled ? 'text-indigo-700' : 'text-slate-600'">失败自动重试</span>
          </label>
        </div>
      </div>

      <!-- 分区：后端节点 -->
      <div>
        <div class="mb-3 flex items-center justify-between">
          <h4 class="text-xs font-semibold uppercase tracking-wide text-slate-400">后端节点</h4>
          <AppButton variant="secondary" size="sm" @click="addNode">
            <svg class="mr-0.5 h-3 w-3" viewBox="0 0 20 20" fill="currentColor" aria-hidden="true">
              <path d="M10 3a1 1 0 011 1v5h5a1 1 0 110 2h-5v5a1 1 0 11-2 0v-5H4a1 1 0 110-2h5V4a1 1 0 011-1z" />
            </svg>
            添加节点
          </AppButton>
        </div>
        <div class="space-y-2">
          <div
            v-for="(node, index) in form.nodes"
            :key="index"
            class="flex items-center gap-2 rounded-lg border border-slate-200 bg-slate-50/50 p-2 transition hover:border-slate-300"
          >
            <AppInput v-model="node.address" placeholder="host:port，例如 127.0.0.1:9090" />
            <AppInput v-model.number="node.weight" type="number" placeholder="权重" class="w-24" />
            <label
              class="flex shrink-0 cursor-pointer items-center gap-1 rounded-lg border border-slate-300 px-2.5 py-1.5 text-xs transition-all duration-150 hover:bg-slate-50"
              :class="node.enabled ? 'border-emerald-500 bg-emerald-50 text-emerald-700' : 'text-slate-600'"
            >
              <input v-model="node.enabled" type="checkbox" class="hidden accent-indigo-600" />
              {{ node.enabled ? '启用' : '禁用' }}
            </label>
            <button
              type="button"
              class="shrink-0 rounded-lg p-1.5 text-slate-400 transition hover:bg-red-50 hover:text-red-600"
              title="删除节点"
              @click="removeNode(index)"
            >
              <svg class="h-4 w-4" viewBox="0 0 20 20" fill="currentColor" aria-hidden="true">
                <path fill-rule="evenodd" d="M9 2a1 1 0 00-.894.553L7.382 4H4a1 1 0 000 2v10a2 2 0 002 2h8a2 2 0 002-2V6a1 1 0 100-2h-3.382l-.724-1.447A1 1 0 0011 2H9zM7 8a1 1 0 012 0v6a1 1 0 11-2 0V8zm5-1a1 1 0 00-1 1v6a1 1 0 102 0V8a1 1 0 00-1-1z" clip-rule="evenodd" />
              </svg>
            </button>
          </div>
        </div>
      </div>
    </form>

    <template #footer>
      <AppButton variant="secondary" @click="emit('update:open', false)">取消</AppButton>
      <AppButton :loading="saving" @click="save">保存</AppButton>
    </template>
  </AppModal>
</template>
