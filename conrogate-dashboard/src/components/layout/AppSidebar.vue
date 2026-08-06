<script setup lang="ts">
/**
 * 侧边导航栏：品牌区 + 功能菜单。
 * 菜单项与 router 中 DashboardLayout 的子路由一一对应。
 */
const navItems = [
  { name: 'routes', label: '路由管理', to: { name: 'routes' } },
  { name: 'upstreams', label: '上游管理', to: { name: 'upstreams' } },
  { name: 'configs', label: '配置版本', to: { name: 'configs' } },
  { name: 'metrics', label: '指标洞察', to: { name: 'metrics' } },
  { name: 'nodes', label: '节点管理', to: { name: 'nodes' } },
  { name: 'plugins', label: '插件管理', to: { name: 'plugins' } },
  { name: 'audit', label: '审计日志', to: { name: 'audit' } },
] as const
</script>

<template>
  <aside class="flex w-52 shrink-0 flex-col border-r border-slate-200 bg-white">
    <!-- 品牌区 -->
    <div class="flex h-14 items-center gap-2 border-b border-slate-200 px-4">
      <div class="flex h-7 w-7 items-center justify-center rounded-md bg-indigo-600 text-xs font-bold text-white">
        C
      </div>
      <span class="text-sm font-semibold text-slate-800">Conrogate</span>
    </div>

    <!-- 功能菜单 -->
    <nav class="flex-1 space-y-1 p-3">
      <router-link
        v-for="item in navItems"
        :key="item.name"
        :to="item.to"
        class="flex items-center gap-2.5 rounded-md px-3 py-2 text-sm transition"
        active-class="bg-indigo-50 font-medium text-indigo-700"
        :class="$route.name !== item.name ? 'text-slate-600 hover:bg-slate-50' : ''"
      >
        <!-- 激活状态指示点 -->
        <span
          :class="[
            'h-2 w-2 rounded-full',
            $route.name === item.name ? 'bg-indigo-500' : 'bg-slate-300',
          ]"
        />
        {{ item.label }}
      </router-link>
    </nav>

    <div class="border-t border-slate-200 p-3 text-xs text-slate-400">控制面 API 管理台</div>
  </aside>
</template>
