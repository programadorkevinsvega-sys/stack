"use client"

import { api, unwrapList } from "@/lib/api/client"
import type { AdminWallet } from "@/lib/types"

export const walletsApi = {
  get: (userId: string) => api.get<AdminWallet>(`/api/v1/admin/wallets/${userId}`),

  adjust: (userId: string, body: { amount: number; reason: string }) =>
    api.post<AdminWallet>(`/api/v1/admin/wallets/${userId}/adjust`, body),
}
