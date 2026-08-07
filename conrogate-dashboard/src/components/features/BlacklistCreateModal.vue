<script setup lang="ts">
/**
 * 拉黑 IP / CIDR 弹窗。
 *
 * 用法：
 *   <BlacklistCreateModal v-model:open="open" @created="reload" />
 */
import { ref, watch } from 'vue'
import { securityApi } from '@/api/security'
import { useToastStore } from '@/stores/toast'
import AppButton from '@/components/ui/AppButton.vue'
import AppInput from '@/components/ui/AppInput.vue'
import AppModal from '@/components/ui/AppModal.vue'
import type { CreateIpBlacklistPayload } from '@/types'

const props = defineProps<{
  open: boolean
}>()

const emit = defineEmits<{
  'update:open': [value: boolean]
  /** 拉黑成功（父级可在此刷新列表） */
  created: []
}>()

const toast = useToastStore()

const formIp = ref('')
const formReason = ref('')
const formExpires = ref('')
const submitting = ref(false)

// 每次打开时清空表单
watch(
  () => props.open,
  (v) => {
    if (v) {
      formIp.value = ''
      formReason.value = ''
      formExpires.value = ''
    }
  },
)

/** 解析时长输入（秒）；空串返回 null */
function parseExpires(raw: string): number | null {
  if (!raw.trim()) return null
  const n = Number(raw)
  return Number.isFinite(n) ? n : NaN
}

function fmtDuration(secs: number): string {
  if (secs >= 86400) return `${Math.floor(secs / 86400)} 天`
  if (secs >= 3600) return `${Math.floor(secs / 3600)} 小时`
  return `${secs} 秒`
}

async function submit(): Promise<void> {
  const ip = formIp.value.trim()
  if (!ip) {
    toast.error('请输入要拉黑的 IP 或 CIDR 网段')
    return
  }
  const expires = parseExpires(formExpires.value)
  if (expires !== null && Number.isNaN(expires)) {
    toast.error('拉黑时长必须是数字（秒）')
    return
  }
  if (expires !== null && expires <= 0) {
    toast.error('拉黑时长必须大于 0 秒，不填则为永久拉黑')
    return
  }
  submitting.value = true
  try {
    const payload: CreateIpBlacklistPayload = {
      ip_or_cidr: ip,
      reason: formReason.value.trim() || null,
      expires_in_seconds: expires,
    }
    await securityApi.create(payload)
    toast.success(`已拉黑 ${ip}`)
    emit('created')
    emit('update:open', false)
  } catch (e) {
    toast.error((e as Error).message)
  } finally {
    submitting.value = false
  }
}
</script>

<template>
  <AppModal :open="open" title="拉黑 IP / CIDR" @close="emit('update:open', false)">
    <div class="space-y-4">
      <AppInput
        v-model="formIp"
        label="IP / 网段"
        placeholder="如 1.2.3.4、10.0.0.0/24、2001:db8::/32"
        required
      />
      <AppInput v-model="formReason" label="原因" placeholder="拉黑原因 / 备注" />
      <AppInput
        v-model="formExpires"
        label="时长（秒，可选）"
        type="number"
        placeholder="不填 = 永久拉黑"
      />
      <p class="text-xs text-slate-500">
        拉黑后数据面数秒内生效，对 HTTP / WebSocket / TCP 隧道三协议统一拦截（403）。
        重复拉黑同一 IP / 网段会刷新原因与过期时间。
        <template v-if="parseExpires(formExpires) !== null && !Number.isNaN(parseExpires(formExpires))">
          <br />本次拉黑时长：{{ fmtDuration(parseExpires(formExpires) as number) }}
        </template>
      </p>
    </div>
    <template #footer>
      <AppButton variant="secondary" @click="emit('update:open', false)">取消</AppButton>
      <AppButton :loading="submitting" @click="submit">确认拉黑</AppButton>
    </template>
  </AppModal>
</template>
