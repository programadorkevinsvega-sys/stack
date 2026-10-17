"use client"

import { api, unwrapList } from "@/lib/api/client"
import type { Plan } from "@/lib/types"

function featureLabels(value: unknown): string[] {
  if (Array.isArray(value)) {
    return value.filter((feature): feature is string => typeof feature === "string")
  }
  if (typeof value === "string") {
    try {
      return featureLabels(JSON.parse(value) as unknown)
    } catch {
      return value.trim() ? [value] : []
    }
  }
  if (value !== null && typeof value === "object") {
    return Object.entries(value).map(([key, entry]) => {
      if (typeof entry === "boolean") return entry ? key : `${key}: no`
      if (typeof entry === "string" || typeof entry === "number") return `${key}: ${entry}`
      return key
    })
  }
  return []
}

function normalizePlan(value: unknown): Plan | null {
  if (value === null || typeof value !== "object") return null
  const raw = value as Record<string, unknown>
  const id = typeof raw.id === "string" ? raw.id : ""
  const name = typeof raw.name === "string" ? raw.name : ""
  if (!id || !name) return null

  const numericPrice = typeof raw.price === "number" ? raw.price : Number(raw.price_amount ?? 0)
  const interval = raw.interval === "year" || raw.billing_cycle === "year" ? "year" : "month"
  const slug = typeof raw.slug === "string" ? raw.slug : ""

  return {
    id,
    name,
    description: typeof raw.description === "string" ? raw.description : "",
    price: Number.isFinite(numericPrice) ? numericPrice : 0,
    currency: typeof raw.currency === "string" ? raw.currency : "USD",
    trial_days: typeof raw.trial_days === "number" ? raw.trial_days : Number(raw.trial_days ?? 0),
    interval,
    features: featureLabels(raw.features ?? raw.features_json),
    is_active: typeof raw.is_active === "boolean" ? raw.is_active : true,
    highlighted: typeof raw.highlighted === "boolean" ? raw.highlighted : slug === "pro",
  }
}

export const plansApi = {
  list: async () => {
    const response = await api.get<unknown>("/api/v1/plans")
    const payload = response !== null && typeof response === "object"
      ? response as Record<string, unknown>
      : null
    const entries = unwrapList(
      Array.isArray(response)
        ? response
        : payload !== null && Array.isArray(payload.data)
          ? payload.data
          : [],
    )
    return entries.flatMap((entry) => {
      const plan = normalizePlan(entry)
      return plan ? [plan] : []
    })
  },
}
