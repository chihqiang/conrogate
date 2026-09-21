<script setup lang="ts">
/**
 * 主布局：左侧导航 + 右侧（顶栏 + 路由出口）。
 * 所有业务页面均为其子路由。
 */
import AppSidebar from '@/components/layout/AppSidebar.vue'
import AppTopbar from '@/components/layout/AppTopbar.vue'
</script>

<template>
  <div class="flex h-screen overflow-hidden bg-slate-100">
    <AppSidebar />
    <div class="flex min-w-0 flex-1 flex-col">
      <AppTopbar />
      <main class="flex-1 overflow-y-auto p-6">
        <router-view />
      </main>
    </div>
  </div>
</template>
