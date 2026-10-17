"use client"

import { api, unwrapList } from "@/lib/api/client"
import type { PaymentMethod } from "@/lib/types"

export const paymentMethodsApi = {
  list: async () => unwrapList(await api.get<PaymentMethod[] | { data: PaymentMethod[] }>("/api/v1/payment-methods")),
  setDefault: (id: string) =>
    api.put<{ ok: true }>(`/api/v1/payment-methods/${id}/default`),
}
