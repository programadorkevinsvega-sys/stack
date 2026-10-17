"use client"

import { api, unwrapList } from "@/lib/api/client"
import type { AdminUser } from "@/lib/types"

export const usersApi = {
  list: (params?: { status?: string; search?: string; page?: number }) => {
    const query = new URLSearchParams()
    if (params?.status) query.set("status", params.status)
    if (params?.search) query.set("search", params.search)
    if (params?.page) query.set("page", String(params.page))
    const suffix = query.toString() ? `?${query}` : ""
    return api.get<AdminUser[]>(`/api/v1/admin/users${suffix}`).then(unwrapList)
  },

  get: (id: string) => api.get<AdminUser>(`/api/v1/admin/users/${id}`),

  disable: (id: string) => api.post<{ ok: true }>(`/api/v1/admin/users/${id}/disable`),

  unlock: (id: string) => api.post<{ ok: true }>(`/api/v1/admin/users/${id}/unlock`),
  activate: (id: string) => api.post<{ ok: true }>(`/api/v1/admin/users/${id}/unlock`),
  delete: (id: string) => api.post<{ ok: true }>(`/api/v1/admin/users/${id}/delete`),
}
