"use client"

import { Check } from "lucide-react"
import type { PaymentProvider } from "@/lib/types"
import { cn } from "@/lib/utils"

interface ProviderInfo {
  id: PaymentProvider
  name: string
  methods: string
  // Brand accent color used for the mark.
  color: string
}

export const PROVIDERS: ProviderInfo[] = [
  { id: "paypal", name: "PayPal", methods: "Saldo y tarjetas PayPal", color: "#0070ba" },
]

// Small brand mark: initial letter chip in the brand color.
export function ProviderMark({
  provider,
  className,
}: {
  provider: PaymentProvider | "card"
  className?: string
}) {
  if (provider === "card") {
    return (
      <span
        className={cn(
          "inline-flex h-8 w-11 items-center justify-center rounded-md bg-muted text-[10px] font-semibold text-muted-foreground",
          className,
        )}
      >
        CARD
      </span>
    )
  }
  const info = PROVIDERS.find((p) => p.id === provider)
  return (
    <span
      className={cn(
        "inline-flex h-8 w-11 items-center justify-center rounded-md text-xs font-bold text-white",
        className,
      )}
      style={{ backgroundColor: info?.color ?? "#666" }}
      aria-hidden="true"
    >
      {info ? info.name.slice(0, 2) : "?"}
    </span>
  )
}

export function PaymentProviderSelector({
  value,
  onChange,
}: {
  value: PaymentProvider | null
  onChange: (p: PaymentProvider) => void
}) {
  return (
    <div role="radiogroup" aria-label="Método de pago" className="grid gap-2 sm:grid-cols-2">
      {PROVIDERS.map((p) => {
        const selected = value === p.id
        return (
          <button
            key={p.id}
            type="button"
            role="radio"
            aria-checked={selected}
            onClick={() => onChange(p.id)}
            className={cn(
              "flex items-center gap-3 rounded-lg border p-3 text-left transition-colors",
              selected
                ? "border-primary bg-accent"
                : "border-border hover:border-primary/50 hover:bg-accent/50",
            )}
          >
            <ProviderMark provider={p.id} />
            <span className="min-w-0 flex-1">
              <span className="block text-sm font-medium">{p.name}</span>
              <span className="block truncate text-xs text-muted-foreground">{p.methods}</span>
            </span>
            <span
              className={cn(
                "flex size-5 shrink-0 items-center justify-center rounded-full border",
                selected ? "border-primary bg-primary text-primary-foreground" : "border-border",
              )}
            >
              {selected && <Check className="size-3" />}
            </span>
          </button>
        )
      })}
    </div>
  )
}
