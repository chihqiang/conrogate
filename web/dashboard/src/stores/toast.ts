/**
 * 全局轻提示（Toast）状态。
 * 通过 `success/error/info` 推送消息，自动超时消失。
 */
import { ref } from 'vue'
import { defineStore } from 'pinia'

export interface ToastItem {
  id: number
  type: 'success' | 'error' | 'info'
  message: string
}

export const useToastStore = defineStore('toast', () => {
  /** 当前可见的 toast 列表 */
  const toasts = ref<ToastItem[]>([])

  let seq = 0

  /** 推送一条 toast；默认 3s 后自动消失 */
  function push(type: ToastItem['type'], message: string, durationMs = 3000): void {
    const id = ++seq
    toasts.value.push({ id, type, message })
    setTimeout(() => dismiss(id), durationMs)
  }

  /** 手动关闭某条 toast */
  function dismiss(id: number): void {
    toasts.value = toasts.value.filter((t) => t.id !== id)
  }

  const success = (msg: string) => push('success', msg)
  const error = (msg: string) => push('error', msg)
  const info = (msg: string) => push('info', msg)

  return { toasts, push, dismiss, success, error, info }
})
