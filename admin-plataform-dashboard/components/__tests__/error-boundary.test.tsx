import { describe, it, expect, vi } from "vitest"
import { render, screen } from "@testing-library/react"
import { ErrorBoundary } from "@/components/error-boundary"

function ThrowingComponent({ message }: { message: string }) {
  throw new Error(message)
}

describe("ErrorBoundary", () => {
  it("renders children when no error", () => {
    render(
      <ErrorBoundary>
        <p>Hello</p>
      </ErrorBoundary>,
    )
    expect(screen.getByText("Hello")).toBeInTheDocument()
  })

  it("renders error UI when child throws", () => {
    vi.spyOn(console, "error").mockImplementation(() => {})

    render(
      <ErrorBoundary>
        <ThrowingComponent message="Boom!" />
      </ErrorBoundary>,
    )

    expect(screen.getByText("Algo salió mal")).toBeInTheDocument()
    expect(screen.getByText("Boom!")).toBeInTheDocument()
  })
})
