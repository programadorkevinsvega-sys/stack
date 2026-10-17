import { describe, it, expect, beforeEach, vi } from "vitest"
import { api, ApiClientError, setRefreshToken, getRefreshToken, setUnauthorizedHandler } from "@/lib/api/client"

const ORIGINAL_ENV = process.env

beforeEach(() => {
  vi.restoreAllMocks()
  setRefreshToken(null)
  document.cookie.split(";").forEach((c) => {
    document.cookie = c.replace(/=.*/, "=; path=/; Max-Age=0")
  })
})

function mockFetch(status: number, body: unknown, headers?: Record<string, string>) {
  const resHeaders = new Headers({ ...(status === 204 ? {} : { "content-type": "application/json" }), ...headers })
  const responseBody = body !== null ? JSON.stringify(body) : undefined
  return vi.spyOn(globalThis, "fetch").mockResolvedValue(
    new Response(responseBody, { status, headers: resHeaders }),
  )
}

describe("api.get", () => {
  it("makes a GET request and returns parsed JSON", async () => {
    mockFetch(200, { ok: true })
    const result = await api.get("/api/v1/test")
    expect(result).toEqual({ ok: true })
  })

  it("throws ApiClientError on non-2xx", async () => {
    mockFetch(500, { message: "Server error" })
    try {
      await api.get("/api/v1/error")
      expect.fail("should have thrown")
    } catch (e) {
      expect(e).toBeInstanceOf(ApiClientError)
      expect((e as ApiClientError).message).toBe("Server error")
    }
  })

  it("includes Accept header", async () => {
    const fetchSpy = mockFetch(200, {})
    await api.get("/api/v1/test")
    const headers = fetchSpy.mock.calls[0][1]?.headers as Record<string, string>
    expect(headers["Accept"]).toBe("application/json")
  })

  it("includes x-refresh-token when set", async () => {
    setRefreshToken("rt-123")
    const fetchSpy = mockFetch(200, {})
    await api.get("/api/v1/test")
    const headers = fetchSpy.mock.calls[0][1]?.headers as Record<string, string>
    expect(headers["x-refresh-token"]).toBe("rt-123")
  })

  it("does not include x-refresh-token when not set", async () => {
    const fetchSpy = mockFetch(200, {})
    await api.get("/api/v1/test")
    const headers = fetchSpy.mock.calls[0][1]?.headers as Record<string, string>
    expect(headers["x-refresh-token"]).toBeUndefined()
  })

  it("reads new refresh token from response header", async () => {
    mockFetch(200, {}, { "x-refresh-token": "new-rt" })
    await api.get("/api/v1/test")
    expect(getRefreshToken()).toBe("new-rt")
  })

  it("calls onUnauthorized on 401", async () => {
    const handler = vi.fn()
    setUnauthorizedHandler(handler)
    mockFetch(401, { message: "Unauthorized" })
    await expect(api.get("/api/v1/protected")).rejects.toThrow(ApiClientError)
    expect(handler).toHaveBeenCalledOnce()
  })

  it("skips 401 handler when silent401 is true", async () => {
    const handler = vi.fn()
    setUnauthorizedHandler(handler)
    mockFetch(401, { message: "Unauthorized" })
    await expect(api.get("/api/v1/protected", { silent401: true })).rejects.toThrow(ApiClientError)
    expect(handler).not.toHaveBeenCalled()
  })
})

describe("api.post", () => {
  it("sends JSON body with Content-Type header", async () => {
    const fetchSpy = mockFetch(201, { ok: true })
    await api.post("/api/v1/create", { name: "test" })
    const init = fetchSpy.mock.calls[0][1]!
    const headers = init.headers as Record<string, string>
    expect(headers["Content-Type"]).toBe("application/json")
    expect(init.body).toBe(JSON.stringify({ name: "test" }))
  })

  it("sends X-CSRF-Token on mutations when csrf_token cookie exists", async () => {
    document.cookie = "csrf_token=my-csrf; path=/"
    const fetchSpy = mockFetch(200, {})
    await api.post("/api/v1/create", {})
    const headers = fetchSpy.mock.calls[0][1]?.headers as Record<string, string>
    expect(headers["X-CSRF-Token"]).toBe("my-csrf")
  })

  it("retries once on 403 with new CSRF token", async () => {
    document.cookie = "csrf_token=first-csrf; path=/"
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      new Response(JSON.stringify({ message: "Forbidden" }), { status: 403 }),
    ).mockResolvedValueOnce(
      new Response(JSON.stringify({ ok: true }), { status: 200 }),
    )
    const result = await api.post("/api/v1/create", {})
    expect(result).toEqual({ ok: true })
    expect(fetchSpy).toHaveBeenCalledTimes(2)
  })

  it("returns undefined for 204", async () => {
    mockFetch(204, undefined)
    const result = await api.post("/api/v1/no-content")
    expect(result).toBeUndefined()
  })

  it("throws with field-level errors on 422", async () => {
    mockFetch(422, {
      error: true,
      message: "Datos inválidos",
      fields: { email: "Este correo ya está registrado" },
    })
    try {
      await api.post("/api/v1/register", { email: "existing@test.com" })
    } catch (e) {
      expect(e).toBeInstanceOf(ApiClientError)
      expect((e as ApiClientError).fields).toEqual({
        email: "Este correo ya está registrado",
      })
    }
  })
})

describe("api.put / patch / delete", () => {
  it("PUT sends the correct method", async () => {
    const fetchSpy = mockFetch(200, {})
    await api.put("/api/v1/update", { key: "value" })
    expect(fetchSpy.mock.calls[0][1]?.method).toBe("PUT")
  })

  it("PATCH sends the correct method", async () => {
    const fetchSpy = mockFetch(200, {})
    await api.patch("/api/v1/update", { key: "value" })
    expect(fetchSpy.mock.calls[0][1]?.method).toBe("PATCH")
  })

  it("DELETE sends the correct method", async () => {
    const fetchSpy = mockFetch(200, {})
    await api.delete("/api/v1/resource")
    expect(fetchSpy.mock.calls[0][1]?.method).toBe("DELETE")
  })
})
