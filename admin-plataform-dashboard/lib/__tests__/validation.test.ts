import { describe, it, expect } from "vitest"
import { loginSchema, createPlanSchema, adjustWalletSchema } from "@/lib/validation"

describe("loginSchema", () => {
  it("accepts valid email and password", () => {
    const result = loginSchema.safeParse({ email: "admin@example.com", password: "MyPass123!@#" })
    expect(result.success).toBe(true)
  })

  it("rejects invalid email", () => {
    const result = loginSchema.safeParse({ email: "not-an-email", password: "MyPass123!@#" })
    expect(result.success).toBe(false)
    if (!result.success) {
      expect(result.error.issues[0].path).toContain("email")
    }
  })

  it("rejects empty password", () => {
    const result = loginSchema.safeParse({ email: "admin@example.com", password: "" })
    expect(result.success).toBe(false)
  })

  it("accepts valid mfa_code when provided", () => {
    const result = loginSchema.safeParse({
      email: "admin@example.com",
      password: "MyPass123!@#",
      mfa_code: "123456",
    })
    expect(result.success).toBe(true)
  })

  it("rejects invalid mfa_code format", () => {
    const result = loginSchema.safeParse({
      email: "admin@example.com",
      password: "MyPass123!@#",
      mfa_code: "abc123",
    })
    expect(result.success).toBe(false)
  })
})

describe("createPlanSchema", () => {
  it("accepts valid plan data", () => {
    const result = createPlanSchema.safeParse({
      name: "Pro",
      description: "Plan profesional",
      price: 29,
      currency: "USD",
      interval: "month",
      features: ["Feature A", "Feature B"],
    })
    expect(result.success).toBe(true)
  })

  it("rejects empty name", () => {
    const result = createPlanSchema.safeParse({
      name: "",
      price: 29,
      currency: "USD",
      interval: "month",
    })
    expect(result.success).toBe(false)
  })

  it("rejects negative price", () => {
    const result = createPlanSchema.safeParse({
      name: "Pro",
      price: -10,
      currency: "USD",
      interval: "month",
    })
    expect(result.success).toBe(false)
  })

  it("rejects invalid interval", () => {
    const result = createPlanSchema.safeParse({
      name: "Pro",
      price: 29,
      currency: "USD",
      interval: "decade",
    })
    expect(result.success).toBe(false)
  })
})

describe("adjustWalletSchema", () => {
  it("accepts valid adjustment", () => {
    const result = adjustWalletSchema.safeParse({ amount: 100, reason: "Customer refund" })
    expect(result.success).toBe(true)
  })

  it("rejects empty reason", () => {
    const result = adjustWalletSchema.safeParse({ amount: 100, reason: "" })
    expect(result.success).toBe(false)
  })

  it("rejects NaN amount", () => {
    const result = adjustWalletSchema.safeParse({ amount: "abc", reason: "test" })
    expect(result.success).toBe(false)
  })
})
