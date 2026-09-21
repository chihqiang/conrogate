<script setup lang="ts">
/**
 * 根组件：只负责承载路由出口与全局 Toast 层。
 * 布局（侧边栏/顶栏）见 views/DashboardLayout.vue。
 */
import AppToast from '@/components/ui/AppToast.vue'
</script>

<template>
  <router-view />
  <AppToast />
</template>
