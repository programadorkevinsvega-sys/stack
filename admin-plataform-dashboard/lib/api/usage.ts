"use client"

import { api } from "@/lib/api/client"
import type { UsageSummary } from "@/lib/types"

export interface UserUsageSummary {
  total_requests: number
  total_input_tokens: number
  total_output_tokens: number
  total_cached_tokens: number
  error_count: number
  error_rate: number
  avg_latency_ms: number
}

export const usageApi = {
  summary: () => api.get<UsageSummary>("/api/v1/admin/usage/summary"),
  byUser: (userId: string) => api.get<UserUsageSummary>(`/api/v1/admin/usage/users/${userId}`),

  refreshViews: () => api.post<{ ok: true }>("/api/v1/admin/usage/refresh-views"),

  createPartition: () => api.post<{ ok: true }>("/api/v1/admin/usage/create-partition"),
}
