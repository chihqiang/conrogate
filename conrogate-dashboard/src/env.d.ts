/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** 控制面 API 基础路径（含 /api/v1 前缀） */
  readonly VITE_API_BASE?: string
  /** 仅本地开发：Vite 代理转发目标 */
  readonly VITE_PROXY_TARGET?: string
}

interface ImportMeta {
  readonly env: ImportMetaEnv
}
