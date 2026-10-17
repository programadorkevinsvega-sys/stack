import { describe, it, expect } from "vitest"
import {
  loginSchema,
  registerSchema,
  createApiKeySchema,
  topUpSchema,
  profileSchema,
  changePasswordSchema,
} from "@/lib/validation"

describe("loginSchema", () => {
  it("accepts valid email and password", () => {
    const result = loginSchema.safeParse({ email: "test@example.com", password: "MyPass123!@#" })
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
    const result = loginSchema.safeParse({ email: "test@example.com", password: "" })
    expect(result.success).toBe(false)
  })

  it("accepts valid mfa_code when provided", () => {
    const result = loginSchema.safeParse({
      email: "test@example.com",
      password: "MyPass123!@#",
      mfa_code: "123456",
    })
    expect(result.success).toBe(true)
  })

  it("rejects invalid mfa_code format", () => {
    const result = loginSchema.safeParse({
      email: "test@example.com",
      password: "MyPass123!@#",
      mfa_code: "abc123",
    })
    expect(result.success).toBe(false)
  })
})

describe("registerSchema", () => {
  it("accepts valid registration", () => {
    const result = registerSchema.safeParse({
      email: "test@example.com",
      password: "MyPass123!@#",
      confirm_password: "MyPass123!@#",
    })
    expect(result.success).toBe(true)
  })

  it("rejects weak password (too short)", () => {
    const result = registerSchema.safeParse({
      email: "test@example.com",
      password: "Short1!",
      confirm_password: "Short1!",
    })
    expect(result.success).toBe(false)
  })

  it("rejects weak password (no uppercase)", () => {
    const result = registerSchema.safeParse({
      email: "test@example.com",
      password: "alllowercase123!",
      confirm_password: "alllowercase123!",
    })
    expect(result.success).toBe(false)
  })

  it("rejects mismatched passwords", () => {
    const result = registerSchema.safeParse({
      email: "test@example.com",
      password: "MyPass123!@#",
      confirm_password: "DifferentPass123!@#",
    })
    expect(result.success).toBe(false)
    if (!result.success) {
      expect(result.error.issues[0].path).toContain("confirm_password")
    }
  })

  it("accepts optional display_name", () => {
    const result = registerSchema.safeParse({
      email: "test@example.com",
      password: "MyPass123!@#",
      confirm_password: "MyPass123!@#",
      display_name: "Test User",
    })
    expect(result.success).toBe(true)
  })

  it("rejects display_name longer than 100 chars", () => {
    const result = registerSchema.safeParse({
      email: "test@example.com",
      password: "MyPass123!@#",
      confirm_password: "MyPass123!@#",
      display_name: "A".repeat(101),
    })
    expect(result.success).toBe(false)
  })
})

describe("createApiKeySchema", () => {
  it("accepts valid name", () => {
    const result = createApiKeySchema.safeParse({ name: "Production" })
    expect(result.success).toBe(true)
  })

  it("rejects empty name", () => {
    const result = createApiKeySchema.safeParse({ name: "" })
    expect(result.success).toBe(false)
  })
})

describe("topUpSchema", () => {
  it("accepts valid positive amount", () => {
    const result = topUpSchema.safeParse({ amount: 50, currency: "USD" })
    expect(result.success).toBe(true)
  })

  it("rejects zero amount", () => {
    const result = topUpSchema.safeParse({ amount: 0, currency: "USD" })
    expect(result.success).toBe(false)
  })

  it("rejects negative amount", () => {
    const result = topUpSchema.safeParse({ amount: -10, currency: "USD" })
    expect(result.success).toBe(false)
  })

  it("rejects more than 2 decimal places", () => {
    const result = topUpSchema.safeParse({ amount: 10.999, currency: "USD" })
    expect(result.success).toBe(false)
  })
})

describe("profileSchema", () => {
  it("accepts empty values", () => {
    const result = profileSchema.safeParse({})
    expect(result.success).toBe(true)
  })

  it("accepts valid display_name", () => {
    const result = profileSchema.safeParse({ display_name: "New Name" })
    expect(result.success).toBe(true)
  })

  it("rejects display_name > 100 chars", () => {
    const result = profileSchema.safeParse({ display_name: "A".repeat(101) })
    expect(result.success).toBe(false)
  })
})

describe("changePasswordSchema", () => {
  it("accepts valid password change", () => {
    const result = changePasswordSchema.safeParse({
      old_password: "OldPass123!@#",
      new_password: "NewPass123!@#",
      confirm_password: "NewPass123!@#",
    })
    expect(result.success).toBe(true)
  })

  it("rejects mismatched new passwords", () => {
    const result = changePasswordSchema.safeParse({
      old_password: "OldPass123!@#",
      new_password: "NewPass123!@#",
      confirm_password: "DifferentPass123!@#",
    })
    expect(result.success).toBe(false)
  })

  it("rejects weak new password", () => {
    const result = changePasswordSchema.safeParse({
      old_password: "OldPass123!@#",
      new_password: "weak",
      confirm_password: "weak",
    })
    expect(result.success).toBe(false)
  })
})
