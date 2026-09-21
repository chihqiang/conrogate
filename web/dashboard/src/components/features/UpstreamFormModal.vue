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
    <form class="space-y-4" @submit.prevent="save">
      <div class="grid grid-cols-3 gap-4">
        <AppInput v-model="form.name" label="上游名称" required placeholder="例如 product-api" />
        <AppSelect v-model="form.algorithm" label="负载均衡算法" :options="toOptions(BalancerAlgorithmLabels)" />
        <label class="flex items-end pb-2 text-sm text-slate-600">
          <input v-model="form.retryEnabled" type="checkbox" class="mr-1.5 accent-indigo-600" />
          失败自动重试
        </label>
      </div>

      <!-- 节点编辑器 -->
      <div>
        <div class="mb-2 flex items-center justify-between">
          <span class="text-sm font-medium text-slate-700">后端节点</span>
          <AppButton variant="secondary" size="sm" @click="addNode">添加节点</AppButton>
        </div>
        <div class="space-y-2">
          <div
            v-for="(node, index) in form.nodes"
            :key="index"
            class="grid grid-cols-[1fr_90px_70px_32px] items-center gap-2"
          >
            <AppInput v-model="node.address" placeholder="host:port，例如 127.0.0.1:9090" />
            <AppInput v-model.number="node.weight" label="" type="number" placeholder="权重" />
            <label class="flex items-center gap-1 text-xs text-slate-600">
              <input v-model="node.enabled" type="checkbox" class="accent-indigo-600" />
              启用
            </label>
            <AppButton
              variant="ghost"
              size="sm"
              class="text-red-500 hover:bg-red-50"
              @click="removeNode(index)"
            >
              删
            </AppButton>
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
