"use client"

import { api, unwrapList } from "@/lib/api/client"
import type { AdminInvoice, AdminInvoiceDetail } from "@/lib/types"

export const invoicesApi = {
  list: (params?: { status?: string; user_id?: string; page?: number }) =>
    api.get<AdminInvoice[]>("/api/v1/admin/invoices", { ...params as any }).then(unwrapList),

  get: (id: string) => api.get<AdminInvoiceDetail>(`/api/v1/admin/invoices/${id}`),

  void: (id: string) => api.post<{ ok: true }>(`/api/v1/admin/invoices/${id}/void`),

  markUncollectible: (id: string) =>
    api.post<{ ok: true }>(`/api/v1/admin/invoices/${id}/uncollectible`),
}
