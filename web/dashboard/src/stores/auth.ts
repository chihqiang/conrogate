/**
 * 鉴权状态（Pinia）。
 *
 * 控制面鉴权模型：请求头携带 `Authorization: Bearer <operator>:<secret>:<role>`，
 * 与后端 `CONROGATE_CONTROL_AUTH_TOKEN` 中的某个 token 完全一致即通过。
 * token 由用户在登录页输入，保存在 localStorage（见 api/client.ts 的 TOKEN_KEY）。
 */
import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import { clearToken, getToken, setToken } from '@/api/client'
import { Role, RoleLabels } from '@/types/enums'

export const useAuthStore = defineStore('auth', () => {
  /** 当前登录 token（operator:secret:role 三段式） */
  const token = ref(getToken())

  /** 从 token 第三段解析角色；非合法角色时视为 viewer（与后端回退策略一致） */
  function parseRole(raw: string): Role {
    const role = raw.split(':').pop() ?? ''
    return role === Role.Operator || role === Role.Admin ? role : Role.Viewer
  }

  /** 是否已登录（有 token 即视为已登录） */
  const isAuthed = computed(() => token.value.length > 0)

  /** 当前角色 */
  const role = computed(() => (isAuthed.value ? parseRole(token.value) : null))

  /** 是否具备写权限（operator / admin） */
  const canWrite = computed(() => role.value === Role.Operator || role.value === Role.Admin)

  /** 登录：保存 token 并解析角色 */
  function login(raw: string): void {
    const trimmed = raw.trim()
    token.value = trimmed
    setToken(trimmed)
  }

  /** 登出：清除本地状态 */
  function logout(): void {
    token.value = ''
    clearToken()
  }

  /** 角色中文名（顶部栏展示用） */
  const roleLabel = computed(() => (role.value ? RoleLabels[role.value] : '-'))

  return { token, role, roleLabel, isAuthed, canWrite, login, logout }
})
