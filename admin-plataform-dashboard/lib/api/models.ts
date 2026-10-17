"use client"

import { api, unwrapList } from "@/lib/api/client"
import type { AdminModel } from "@/lib/types"

export const modelsApi = {
  listAccounts: () => api.get<Array<{
    account_id: string
    account_alias: string
    aws_account_id: string
    region: string
    auth_method: string
    is_healthy: boolean
    is_active: boolean
  }>>('/api/v1/admin/accounts').then((response) => Array.isArray(response) ? response : (response as { data: Array<{
    account_id: string
    account_alias: string
    aws_account_id: string
    region: string
    auth_method: string
    is_healthy: boolean
    is_active: boolean
  }> }).data),
  list: (params?: { provider?: string; active?: boolean; search?: string }) =>
    api.get<AdminModel[]>("/api/v1/admin/models", { ...params as any }).then(unwrapList),

  get: (id: string) => api.get<AdminModel>(`/api/v1/admin/models/${id}`),

  create: (body: Partial<AdminModel>) =>
    api.post<AdminModel>("/api/v1/admin/models", body),

  update: (id: string, body: Partial<AdminModel>) =>
    api.patch<AdminModel>(`/api/v1/admin/models/${id}`, body),

  upsertMantleConnection: (accountId: string, modelId: string, body: {
    api_key: string
    base_url: string
    expires_at?: string
  }) => api.post(`/api/v1/admin/accounts/${accountId}/models/${modelId}/mantle-connection`, body),

  deprecate: (id: string) =>
    api.post<{ ok: true }>(`/api/v1/admin/models/${id}/deprecate`),
}
