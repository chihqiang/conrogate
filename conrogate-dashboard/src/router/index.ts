/**
 * 应用路由。
 * 除 /login 外均为受保护页面：未登录访问时重定向到登录页（记录回跳地址）。
 */
import { createRouter, createWebHistory } from 'vue-router'
import { useAuthStore } from '@/stores/auth'

const router = createRouter({
  history: createWebHistory(import.meta.env.BASE_URL),
  routes: [
    {
      path: '/login',
      name: 'login',
      component: () => import('@/views/LoginView.vue'),
      // 公开页面：无需登录即可访问
      meta: { public: true },
    },
    {
      path: '/',
      component: () => import('@/views/DashboardLayout.vue'),
      // 默认进入路由管理页
      redirect: { name: 'routes' },
      children: [
        { path: 'routes', name: 'routes', component: () => import('@/views/RoutesView.vue') },
        { path: 'upstreams', name: 'upstreams', component: () => import('@/views/UpstreamsView.vue') },
        { path: 'configs', name: 'configs', component: () => import('@/views/ConfigsView.vue') },
        { path: 'metrics', name: 'metrics', component: () => import('@/views/MetricsView.vue') },
        { path: 'nodes', name: 'nodes', component: () => import('@/views/NodesView.vue') },
        { path: 'audit', name: 'audit', component: () => import('@/views/AuditView.vue') },
      ],
    },
  ],
})

// 全局导航守卫：登录态校验 + 登录页反向拦截
router.beforeEach((to) => {
  const auth = useAuthStore()
  if (!to.meta.public && !auth.isAuthed) {
    // 未登录访问受保护页：跳登录页并记录来源
    return { name: 'login', query: { redirect: to.fullPath } }
  }
  if (to.name === 'login' && auth.isAuthed) {
    // 已登录仍访问登录页：直接回首页
    return { name: 'routes' }
  }
  return true
})

export { router }
export default router
