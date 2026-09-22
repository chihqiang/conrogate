<script setup lang="ts">
/**
 * 登录页：输入控制面鉴权 token（`operator:secret:role` 三段式）。
 * 与后端 `CONROGATE_CONTROL_AUTH_TOKEN` 中任一 token 完全一致即通过。
 *
 * 安全保障：登录时调用 GET /auth/verify 验证 token，
 * 只有服务端可达且 token 有效才保存 token 并跳转。
 * 服务端未启动、token 无效、网络异常均会被拦截，不会进入控制台。
 */
import { computed, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useAuthStore } from '@/stores/auth'
import { useToastStore } from '@/stores/toast'
import { authApi } from '@/api/auth'
import { ApiError } from '@/api/client'
import AppButton from '@/components/ui/AppButton.vue'
import AppInput from '@/components/ui/AppInput.vue'
import { Role, RoleLabels } from '@/types/enums'

const auth = useAuthStore()
const toast = useToastStore()
const route = useRoute()
const router = useRouter()

const token = ref('')
const submitting = ref(false)

/** 实时预览解析出的角色（token 第三段） */
const rolePreview = computed<string>(() => {
  const role = token.value.split(':').pop() ?? ''
  if (role === Role.Operator || role === Role.Admin) {
    return RoleLabels[role as Role]
  }
  return token.value.length > 0 ? '只读（缺省回退 viewer）' : '-'
})

/**
 * 登录流程：
 * 1. 前端校验非空
 * 2. 临时保存 token（供 API client 携带 Authorization 头）
 * 3. 调用 GET /auth/verify 验证 token 有效性
 * 4. 验证通过 → 正式登录跳转
 * 5. 验证失败 → 清除临时 token，Toast 报错，不跳转
 */
async function submit(): Promise<void> {
  if (!token.value.trim()) {
    toast.error('请输入鉴权 token')
    return
  }

  // 临时保存 token，使验证请求能携带 Authorization 头
  auth.login(token.value)
  submitting.value = true

  try {
    // 调用专属验证端点确认 token 有效性
    await authApi.verify()
    // 验证通过，跳转（优先回跳地址）
    const redirect = typeof route.query.redirect === 'string' ? route.query.redirect : '/'
    await router.push(redirect)
  } catch (err) {
    // 验证失败：清除临时 token，留在登录页
    auth.logout()

    if (err instanceof ApiError) {
      if (err.status === 0) {
        // 网络层错误：服务端未启动 / 连接被拒
        toast.error('无法连接控制面，请确认服务端已启动')
      } else if (err.status === 401 || err.code === 10002) {
        toast.error('Token 无效，请检查输入')
      } else if (err.status === 403 || err.code === 10003) {
        toast.error('Token 权限不足')
      } else {
        toast.error(`验证失败：${err.message}`)
      }
    } else {
      toast.error('验证失败，请重试')
    }
  } finally {
    submitting.value = false
  }
}
</script>

<template>
  <div class="flex min-h-screen items-center justify-center bg-slate-100 px-4">
    <div class="w-full max-w-md rounded-lg border border-slate-200 bg-white p-8 shadow-sm">
      <!-- 品牌区 -->
      <div class="mb-6 flex items-center gap-3">
        <div class="flex h-10 w-10 items-center justify-center rounded-lg bg-indigo-600 text-sm font-bold text-white">
          C
        </div>
        <div>
          <h1 class="text-lg font-semibold text-slate-800">Conrogate Dashboard</h1>
          <p class="text-xs text-slate-400">网关控制面管理台</p>
        </div>
      </div>

      <!-- 表单 -->
      <form class="space-y-4" @submit.prevent="submit">
        <AppInput
          v-model="token"
          label="鉴权 Token"
          type="password"
          required
          placeholder="operator:secret:role"
          autocomplete="off"
        />
        <p class="text-sm text-slate-500">
          角色预览：<span class="font-medium text-indigo-600">{{ rolePreview }}</span>
        </p>
        <AppButton type="submit" :loading="submitting" class="w-full" size="lg">登录</AppButton>
      </form>

      <!-- 说明 -->
      <div class="mt-6 rounded-md bg-slate-50 p-3 text-xs leading-5 text-slate-500">
        <p class="font-medium text-slate-600">Token 获取方式</p>
        <p>由控制面环境变量 <code class="text-indigo-600">CONROGATE_CONTROL_AUTH_TOKEN</code> 配置，
        支持逗号分隔多个 token。格式：<code>operator:secret:role</code>，role 取
        <code>viewer</code>（只读）/ <code>operator</code>（读写）/ <code>admin</code>（完全权限）。</p>
      </div>
    </div>
  </div>
</template>
