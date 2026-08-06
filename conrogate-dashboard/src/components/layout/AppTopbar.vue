<script setup lang="ts">
/**
 * 顶栏：当前页面标题 + 当前角色徽标 + 退出登录。
 */
import { computed } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useAuthStore } from '@/stores/auth'
import AppBadge from '@/components/ui/AppBadge.vue'
import AppButton from '@/components/ui/AppButton.vue'

const route = useRoute()
const router = useRouter()
const auth = useAuthStore()

/** 路由名 → 页面标题 */
const pageTitles: Record<string, string> = {
  routes: '路由管理',
  upstreams: '上游管理',
  configs: '配置版本',
  metrics: '指标洞察',
  nodes: '节点管理',
  audit: '审计日志',
}

const title = computed(() => pageTitles[route.name as string] ?? '控制台')

/** 退出登录：清除 token 回登录页 */
function logout(): void {
  auth.logout()
  void router.push({ name: 'login' })
}
</script>

<template>
  <header class="flex h-14 shrink-0 items-center justify-between border-b border-slate-200 bg-white px-6">
    <h1 class="text-base font-semibold text-slate-800">{{ title }}</h1>
    <div class="flex items-center gap-3">
      <!-- 当前角色（仅 admin/operator 有写权限） -->
      <AppBadge :tone="auth.canWrite ? 'indigo' : 'gray'">{{ auth.roleLabel }}</AppBadge>
      <AppButton variant="ghost" size="sm" @click="logout">退出</AppButton>
    </div>
  </header>
</template>
