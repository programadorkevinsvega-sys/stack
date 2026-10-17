import { describe, it, expect } from "vitest"
import { render, screen } from "@testing-library/react"
import { BrandMark, BrandLogo } from "@/components/brand"

describe("BrandMark", () => {
  it("renders a svg icon", () => {
    const { container } = render(<BrandMark />)
    expect(container.querySelector("svg")).toBeInTheDocument()
  })
})

describe("BrandLogo", () => {
  it("renders the Nexus Admin text", () => {
    render(<BrandLogo />)
    expect(screen.getByText("Nexus Admin")).toBeInTheDocument()
  })
})
