"use client"

import { api, getRefreshToken } from "@/lib/api/client"
import type { ActiveSession, EnableMfaResponse, LoginResponse, User } from "@/lib/types"

export const authApi = {
  register: (body: { email: string; password: string; display_name?: string }) =>
    api.post<{ ok: true }>("/api/v1/auth/register", body),

  verifyEmail: (token: string) =>
    api.post<{ ok: true }>("/api/v1/auth/verify-email", { token }),

  login: (body: { email: string; password: string; mfa_code?: string }) =>
    api.post<LoginResponse>("/api/v1/auth/login", body),

  me: () => api.get<User>("/api/v1/me", { silent401: true }),

  updateProfile: (body: { display_name?: string; avatar_url?: string }) =>
    api.patch<User>("/api/v1/me", body),

  changePassword: (body: { old_password: string; new_password: string }) =>
    api.post<{ ok: true }>("/api/v1/auth/change-password", body),

  enableMfa: () => api.post<EnableMfaResponse>("/api/v1/auth/enable-mfa"),
  confirmMfa: (code: string) => api.post<{ ok: true }>("/api/v1/auth/confirm-mfa", { code }),
  disableMfa: (code: string) => api.post<{ ok: true }>("/api/v1/auth/disable-mfa", { code }),

  logout: () =>
    api.post<{ ok: true }>("/api/v1/auth/logout", { refresh_token: getRefreshToken() }),

  logoutAll: () =>
    api.post<{ ok: true }>("/api/v1/auth/logout-all", { refresh_token: getRefreshToken() }),

  listSessions: () => api.get<{ sessions: ActiveSession[] }>("/api/v1/auth/sessions"),

  revokeSession: (sessionId: string) =>
    api.delete<{ success: true }>(`/api/v1/auth/sessions/${encodeURIComponent(sessionId)}`),
}
