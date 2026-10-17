import { describe, it, expect, vi } from "vitest"
import { render, screen, fireEvent } from "@testing-library/react"
import { PaymentProviderSelector, ProviderMark, PROVIDERS } from "@/components/payment-provider-selector"

describe("PaymentProviderSelector", () => {
  it("renders all providers", () => {
    render(<PaymentProviderSelector value={null} onChange={() => {}} />)
    PROVIDERS.forEach((p) => {
      expect(screen.getByText(p.name)).toBeInTheDocument()
    })
  })

  it("calls onChange when clicked", () => {
    const onChange = vi.fn()
    render(<PaymentProviderSelector value={null} onChange={onChange} />)
    fireEvent.click(screen.getByText("Stripe"))
    expect(onChange).toHaveBeenCalledWith("stripe")
  })

  it("shows selected provider with check mark", () => {
    render(<PaymentProviderSelector value="stripe" onChange={() => {}} />)
    const selected = screen.getByRole("radio", { checked: true })
    expect(selected).toBeInTheDocument()
  })

  it("has radiogroup role", () => {
    render(<PaymentProviderSelector value={null} onChange={() => {}} />)
    expect(screen.getByRole("radiogroup")).toBeInTheDocument()
  })
})

describe("ProviderMark", () => {
  it("renders CARD badge for card provider", () => {
    render(<ProviderMark provider="card" />)
    expect(screen.getByText("CARD")).toBeInTheDocument()
  })
})
