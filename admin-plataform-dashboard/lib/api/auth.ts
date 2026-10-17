"use client"

import { api, getRefreshToken } from "@/lib/api/client"
import type { AdminLoginResponse, AdminUser } from "@/lib/types"

export const authApi = {
  login: (body: { email: string; password: string; mfa_code?: string }) =>
    api.post<AdminLoginResponse>("/api/v1/auth/login", body),

  me: () => api.get<AdminUser>("/api/v1/me", { silent401: true }),

  changePassword: (body: { old_password: string; new_password: string }) =>
    api.post<{ ok: true }>("/api/v1/auth/change-password", body),

  logout: () =>
    api.post<{ ok: true }>("/api/v1/auth/logout", { refresh_token: getRefreshToken() }),
}
