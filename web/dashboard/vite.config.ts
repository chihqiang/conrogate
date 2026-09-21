import { fileURLToPath, URL } from 'node:url'
import { defineConfig, loadEnv } from 'vite'
import vue from '@vitejs/plugin-vue'
import tailwindcss from '@tailwindcss/vite'

/**
 * Vite 配置。
 *
 * - `/api` 前缀在开发环境通过代理转发到控制面（默认 127.0.0.1:9000），
 *   从而规避浏览器跨域限制；生产环境请用反向代理或直接配置 VITE_API_BASE 指向控制面地址。
 */
export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), '')
  const proxyTarget = env.VITE_PROXY_TARGET || 'http://127.0.0.1:9000'

  return {
    plugins: [vue(), tailwindcss()],
    resolve: {
      // 路径别名 @ -> src/，配合 tsconfig.json 的 paths 使用
      alias: {
        '@': fileURLToPath(new URL('./src', import.meta.url)),
      },
    },
    server: {
      port: 5173,
      proxy: {
        '/api': {
          target: proxyTarget,
          changeOrigin: true,
        },
      },
    },
  }
})
