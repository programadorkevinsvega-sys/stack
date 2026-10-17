"use client"

import { api, unwrapList } from "@/lib/api/client"
import type { AdminPlan } from "@/lib/types"

export const plansApi = {
  list: () => api.get<AdminPlan[]>("/api/v1/admin/plans").then(unwrapList),

  get: (id: string) => api.get<AdminPlan>(`/api/v1/admin/plans/${id}`),

  create: (body: Partial<AdminPlan>) =>
    api.post<AdminPlan>("/api/v1/admin/plans", body),

  update: (id: string, body: Partial<AdminPlan>) =>
    api.patch<AdminPlan>(`/api/v1/admin/plans/${id}`, body),

  deactivate: (id: string) =>
    api.post<{ ok: true }>(`/api/v1/admin/plans/${id}/deactivate`),
}
