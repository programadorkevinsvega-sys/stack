"use client"

import { api, unwrapList } from "@/lib/api/client"
import type { Invoice } from "@/lib/types"

function numberValue(value: unknown): number {
  const parsed = typeof value === "number" ? value : Number(value ?? 0)
  return Number.isFinite(parsed) ? parsed : 0
}

function normalizeInvoice(value: unknown): Invoice | null {
  if (value === null || typeof value !== "object") return null
  const raw = value as Record<string, unknown>
  if (typeof raw.id !== "string") return null
  const status = raw.status === "paid" || raw.status === "open" || raw.status === "void" || raw.status === "uncollectible"
    ? raw.status
    : "open"
  const lineItems = Array.isArray(raw.line_items)
    ? raw.line_items.flatMap((entry) => {
        if (entry === null || typeof entry !== "object") return []
        const item = entry as Record<string, unknown>
        return [{
          description: typeof item.description === "string" ? item.description : "",
          quantity: numberValue(item.quantity),
          unit_price: numberValue(item.unit_price),
          amount: numberValue(item.amount),
        }]
      })
    : undefined
  return {
    id: raw.id,
    amount: numberValue(raw.amount ?? raw.total),
    currency: typeof raw.currency === "string" ? raw.currency : "USD",
    status,
    due_date: typeof raw.due_date === "string" ? raw.due_date : "",
    paid_at: typeof raw.paid_at === "string" ? raw.paid_at : null,
    invoice_url: typeof raw.invoice_url === "string" ? raw.invoice_url : typeof raw.pdf_url === "string" ? raw.pdf_url : null,
    line_items: lineItems,
  }
}

export const invoicesApi = {
  list: async () => {
    const response = await api.get<unknown>("/api/v1/invoices")
    const payload = response !== null && typeof response === "object" ? response as Record<string, unknown> : null
    const values = Array.isArray(response) ? response : payload !== null && Array.isArray(payload.data) ? payload.data : []
    return unwrapList(values).flatMap((entry) => {
      const invoice = normalizeInvoice(entry)
      return invoice ? [invoice] : []
    })
  },
  get: (id: string) => api.get<Invoice>(`/api/v1/invoices/${id}`),
}
