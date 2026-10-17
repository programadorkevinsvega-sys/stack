"use client"

import { api, unwrapList } from "@/lib/api/client"
import type { ApiKey, ApiKeyScope, CreatedApiKey } from "@/lib/types"

export const apiKeysApi = {
  list: async () => unwrapList(await api.get<ApiKey[] | { data: ApiKey[] }>("/api/v1/api-keys")),
  create: (name: string, scopes: ApiKeyScope[]) =>
    api.post<CreatedApiKey>("/api/v1/api-keys", { name, scopes }),
  update: (id: string, name: string, scopes: ApiKeyScope[]) =>
    api.patch<ApiKey>(`/api/v1/api-keys/${id}`, { name, scopes }),
  rotate: (id: string, scopes?: ApiKeyScope[]) =>
    api.post<CreatedApiKey>(`/api/v1/api-keys/${id}/rotate`, { scopes }),
  revoke: (id: string) => api.delete<{ ok: true }>(`/api/v1/api-keys/${id}`),
}
