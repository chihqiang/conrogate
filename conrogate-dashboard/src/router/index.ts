/**
 * 应用路由。
 * 除 /login 外均为受保护页面：未登录访问时重定向到登录页（记录回跳地址）。
 *
 * 侧边栏 / 顶栏从本路由表派生，菜单顺序与标题都在此维护。
 */
import { createRouter, createWebHistory } from 'vue-router'
import { useAuthStore } from '@/stores/auth'

declare module 'vue-router' {
  interface RouteMeta {
    /** 页面标题（侧边栏 / 顶栏展示） */
    title?: string
    /** 公开页面：无需登录即可访问 */
    public?: boolean
  }
}

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
      name: 'dashboard',
      component: () => import('@/views/DashboardLayout.vue'),
      // 默认进入指标洞察页
      redirect: { name: 'metrics' },
      children: [
        { path: 'metrics', name: 'metrics', component: () => import('@/views/MetricsView.vue'), meta: { title: '指标洞察' } },
        { path: 'upstreams', name: 'upstreams', component: () => import('@/views/UpstreamsView.vue'), meta: { title: '上游管理' } },
        { path: 'routes', name: 'routes', component: () => import('@/views/RoutesView.vue'), meta: { title: '路由管理' } },
        { path: 'plugins', name: 'plugins', component: () => import('@/views/PluginsView.vue'), meta: { title: '插件管理' } },
        { path: 'configs', name: 'configs', component: () => import('@/views/ConfigsView.vue'), meta: { title: '配置版本' } },
        { path: 'nodes', name: 'nodes', component: () => import('@/views/NodesView.vue'), meta: { title: '节点管理' } },
        { path: 'audit', name: 'audit', component: () => import('@/views/AuditView.vue'), meta: { title: '审计日志' } },
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
