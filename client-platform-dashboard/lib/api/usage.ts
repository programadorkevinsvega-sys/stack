"use client"

import { api } from "@/lib/api/client"
import type { LatencyStats, TopModel, UsageSummary } from "@/lib/types"

function asNumber(value: unknown): number {
  const parsed = typeof value === "number" ? value : Number(value ?? 0)
  return Number.isFinite(parsed) ? parsed : 0
}

function normalizeSummary(value: unknown): UsageSummary {
  const raw = value !== null && typeof value === "object" ? value as Record<string, unknown> : {}
  const series = Array.isArray(raw.series)
    ? raw.series.flatMap((entry) => {
        if (entry === null || typeof entry !== "object") return []
        const point = entry as Record<string, unknown>
        return [{
          date: typeof point.date === "string" ? point.date : "",
          requests: asNumber(point.requests),
          tokens: asNumber(point.tokens),
        }]
      })
    : []
  return {
    requests: asNumber(raw.requests ?? raw.total_requests),
    tokens_in: asNumber(raw.tokens_in ?? raw.total_input_tokens),
    tokens_out: asNumber(raw.tokens_out ?? raw.total_output_tokens),
    tokens_cached: asNumber(raw.tokens_cached ?? raw.total_cached_tokens),
    error_count: asNumber(raw.error_count),
    error_rate: asNumber(raw.error_rate),
    avg_latency_ms: asNumber(raw.avg_latency_ms),
    series,
  }
}

function normalizeTopModel(value: unknown): TopModel {
  const raw = value !== null && typeof value === "object" ? value as Record<string, unknown> : {}
  return {
    model_id: typeof raw.model_id === "string" ? raw.model_id : typeof raw.model_catalog_id === "string" ? raw.model_catalog_id : "",
    display_name: typeof raw.display_name === "string" ? raw.display_name : typeof raw.model_catalog_id === "string" ? raw.model_catalog_id : "Modelo",
    requests: asNumber(raw.requests ?? raw.total_requests),
    tokens: asNumber(raw.tokens ?? asNumber(raw.total_input_tokens) + asNumber(raw.total_output_tokens)),
    cost: asNumber(raw.cost),
  }
}

function normalizeLatency(value: unknown): LatencyStats {
  const raw = value !== null && typeof value === "object" ? value as Record<string, unknown> : {}
  return {
    p50: asNumber(raw.p50 ?? raw.p50_latency_ms),
    p95: asNumber(raw.p95 ?? raw.p95_latency_ms),
    p99: asNumber(raw.p99 ?? raw.p99_latency_ms),
    min: asNumber(raw.min ?? raw.min_latency_ms),
    avg: asNumber(raw.avg ?? raw.avg_latency_ms),
    max: asNumber(raw.max ?? raw.max_latency_ms),
  }
}

export const usageApi = {
  summary: async () => normalizeSummary(await api.get<unknown>("/api/v1/usage")),
  topModels: async () => {
    const response = await api.get<unknown>("/api/v1/usage/top-models")
    const values = Array.isArray(response) ? response : []
    return values.map(normalizeTopModel)
  },
  latency: async () => normalizeLatency(await api.get<unknown>("/api/v1/usage/latency")),
}
