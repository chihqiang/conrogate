/**
 * 公共格式化工具函数。
 */

/** 时间戳格式化（本地时区 YYYY-MM-DD HH:mm:ss）；空值返回 '-' */
export function fmtTime(value: string | null | undefined): string {
  if (!value) return '-'
  const d = new Date(value)
  if (Number.isNaN(d.getTime())) return '-'
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`
}

/** 字节数 → 人类可读（KB/MB/GB） */
export function fmtBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`
}

/** 秒数 → 人类可读时长（天/小时/分钟/秒） */
export function fmtDuration(secs: number): string {
  if (secs >= 86400) return `${Math.floor(secs / 86400)} 天`
  if (secs >= 3600) return `${Math.floor(secs / 3600)} 小时`
  if (secs >= 60) return `${Math.floor(secs / 60)} 分钟`
  return `${secs} 秒`
}
