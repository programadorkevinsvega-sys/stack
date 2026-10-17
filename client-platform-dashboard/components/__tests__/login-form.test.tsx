import { describe, it, expect, vi, beforeEach } from "vitest"
import { render, screen, fireEvent, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import LoginPage from "@/app/(auth)/login/page"

// Mock next/navigation
vi.mock("next/navigation", () => ({
  useRouter: () => ({ push: vi.fn(), replace: vi.fn() }),
  useSearchParams: () => new URLSearchParams(),
}))

// Mock the API client to prevent actual network calls
vi.mock("@/lib/api/auth", () => ({
  authApi: {
    login: vi.fn(),
  },
}))

// Mock sonner toast
vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn(), info: vi.fn() },
}))

describe("LoginPage", () => {
  beforeEach(() => {
    vi.clearAllMocks()
    // Reset cookies
    document.cookie.split(";").forEach((c) => {
      document.cookie = c.replace(/=.*/, "=; path=/; Max-Age=0")
    })
  })

  it("renders the login form", () => {
    render(<LoginPage />)
    expect(screen.getByText("Inicia sesión")).toBeInTheDocument()
    expect(screen.getByLabelText("Correo")).toBeInTheDocument()
    expect(screen.getByLabelText("Contraseña")).toBeInTheDocument()
  })

  it("has a link to register", () => {
    render(<LoginPage />)
    expect(screen.getByText("Regístrate")).toBeInTheDocument()
  })

  it("submits the form with email and password", async () => {
    const user = userEvent.setup()
    const { authApi } = await import("@/lib/api/auth")

    render(<LoginPage />)

    await user.type(screen.getByLabelText("Correo"), "test@example.com")
    await user.type(screen.getByLabelText("Contraseña"), "MyPass123!@#")
    await user.click(screen.getByRole("button", { name: "Iniciar sesión" }))

    await waitFor(() => {
      expect(authApi.login).toHaveBeenCalledWith({
        email: "test@example.com",
        password: "MyPass123!@#",
        mfa_code: undefined,
      })
    })
  })
})
