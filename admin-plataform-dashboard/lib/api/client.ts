"use client"

import { API_BASE } from "@/lib/config"
import type { ApiError } from "@/lib/types"

const MUTATION_METHODS = new Set(["POST", "PUT", "PATCH", "DELETE"])

export class ApiClientError extends Error {
  status: number
  fields?: Record<string, string>
  constructor(status: number, message: string, fields?: Record<string, string>) {
    super(message)
    this.name = "ApiClientError"
    this.status = status
    this.fields = fields
  }
}

function readCookie(name: string): string | null {
  if (typeof document === "undefined") return null
  const match = document.cookie.match(new RegExp("(?:^|; )" + name + "=([^;]*)"))
  return match ? decodeURIComponent(match[1]) : null
}

let onUnauthorized: (() => void) | null = null
export function setUnauthorizedHandler(fn: () => void) {
  onUnauthorized = fn
}

let inMemoryRefreshToken: string | null = null
export function setRefreshToken(token: string | null) {
  inMemoryRefreshToken = token
}
export function getRefreshToken() {
  return inMemoryRefreshToken
}

export function unwrapList<T>(response: T[] | { data: T[] }): T[] {
  return Array.isArray(response) ? response : response.data
}

interface RequestOptions {
  method?: string
  body?: unknown
  silent401?: boolean
}

async function request<T>(path: string, options: RequestOptions = {}): Promise<T> {
  if (!API_BASE) {
    throw new ApiClientError(0, "NEXT_PUBLIC_ADMIN_BFF_URL is not set.")
  }

  const method = options.method ?? "GET"
  const headers: Record<string, string> = { Accept: "application/json" }

  if (options.body !== undefined) {
    headers["Content-Type"] = "application/json"
  }

  if (MUTATION_METHODS.has(method)) {
    const csrf = readCookie("csrf_token")
    if (csrf) headers["X-CSRF-Token"] = csrf
  }

  if (inMemoryRefreshToken) {
    headers["x-refresh-token"] = inMemoryRefreshToken
  }

  const doFetch = () =>
    fetch(`${API_BASE}${path}`, {
      method,
      headers,
      credentials: "include",
      body: options.body !== undefined ? JSON.stringify(options.body) : undefined,
    })

  let res = await doFetch()

  if (res.status === 403 && MUTATION_METHODS.has(method)) {
    const csrf = readCookie("csrf_token")
    if (csrf) headers["X-CSRF-Token"] = csrf
    res = await doFetch()
  }

  if (res.status === 401 && !options.silent401) {
    onUnauthorized?.()
  }

  const headerToken = res.headers.get("x-refresh-token")
  if (headerToken) {
    inMemoryRefreshToken = headerToken
  }

  if (res.status === 204) return undefined as T

  let data: unknown = null
  const text = await res.text()
  if (text) {
    try { data = JSON.parse(text) } catch { data = null }
  }

  if (!res.ok) {
    const err = (data ?? {}) as ApiError
    throw new ApiClientError(res.status, err.message || `Error ${res.status}`, err.fields)
  }

  return data as T
}

export const api = {
  get: <T>(path: string, opts?: RequestOptions) => request<T>(path, { ...opts, method: "GET" }),
  post: <T>(path: string, body?: unknown, opts?: RequestOptions) =>
    request<T>(path, { ...opts, method: "POST", body }),
  put: <T>(path: string, body?: unknown, opts?: RequestOptions) =>
    request<T>(path, { ...opts, method: "PUT", body }),
  patch: <T>(path: string, body?: unknown, opts?: RequestOptions) =>
    request<T>(path, { ...opts, method: "PATCH", body }),
  delete: <T>(path: string, opts?: RequestOptions) => request<T>(path, { ...opts, method: "DELETE" }),
}
