"use client"

import { api, unwrapList } from "@/lib/api/client"
import type { Model } from "@/lib/types"

export const modelsApi = {
  list: async () => unwrapList(await api.get<Model[] | { data: Model[] }>("/api/v1/models")),
  get: (id: string) => api.get<Model>(`/api/v1/models/${id}`),
}
