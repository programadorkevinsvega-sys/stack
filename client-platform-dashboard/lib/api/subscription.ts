"use client"

import { api } from "@/lib/api/client"
import type { PaymentProvider, RedirectResponse, Subscription } from "@/lib/types"

export const subscriptionApi = {
  // Uses silent401 so the protected-layout gate can probe without triggering a
  // redirect loop; a 404 here means "no subscription yet".
  get: () => api.get<Subscription>("/api/v1/subscription", { silent401: true }),
  acceptLegal: () => api.post<{ accepted: boolean; accepted_at: string }>("/api/v1/legal/accept", { terms_version: "2026-07-24", privacy_version: "2026-07-24" }),
  subscribe: (plan_id: string, provider: PaymentProvider) =>
    // NOTE: `provider` is an optional field pending backend confirmation. See TODO-BACKEND.md.
    api.post<RedirectResponse>("/api/v1/subscription", { plan_id, provider, terms_version: "2026-07-24", privacy_version: "2026-07-24", return_url: `${window.location.origin}/subscription/confirm?plan_id=${plan_id}`, cancel_url: `${window.location.origin}/plans` }),
  cancel: () => api.post<{ ok: true }>("/api/v1/subscription/cancel"),
  confirmPayPal: (order_id: string, plan_id: string) => api.post<Subscription>("/api/v1/subscription/paypal/confirm", { order_id, plan_id, terms_version: "2026-07-24", privacy_version: "2026-07-24" }),
  confirmPayPalVault: (setup_token_id: string, plan_id: string) => api.post<Subscription>("/api/v1/subscription/paypal/vault/confirm", { setup_token_id, plan_id, terms_version: "2026-07-24", privacy_version: "2026-07-24" }),
}
