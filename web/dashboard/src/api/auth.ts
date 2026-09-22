/**
 * 鉴权 API：Token 验证。
 */
import { api } from '@/api/client'

/** verify_token 返回体 */
export interface VerifyResult {
  operator: string
  role: 'admin' | 'operator' | 'viewer'
}

export const authApi = {
  /** 验证当前 token 是否有效（受认证中间件保护，通过即代表 token 有效） */
  verify: () => api.get<VerifyResult>('/auth/verify'),
}
