"use client"

import { api } from "@/lib/api/client"
import type { PaymentProvider, RedirectResponse, Wallet } from "@/lib/types"

export const walletApi = {
  get: async () => {
    const value = await api.get<unknown>("/api/v1/wallet")
    const raw = value !== null && typeof value === "object" ? value as Record<string, unknown> : {}
    const numberValue = (candidate: unknown) => {
      const parsed = typeof candidate === "number" ? candidate : Number(candidate ?? 0)
      return Number.isFinite(parsed) ? parsed : 0
    }
    return {
      balance: numberValue(raw.balance),
      currency: typeof raw.currency === "string" ? raw.currency : "USD",
      credit_limit: numberValue(raw.credit_limit),
    } satisfies Wallet
  },
  topUp: (amount: number, currency: string, provider: PaymentProvider) =>
    // NOTE: `provider` is an optional field pending backend confirmation. See TODO-BACKEND.md.
    api.post<RedirectResponse>("/api/v1/top-up", { amount, currency, provider }),
}
