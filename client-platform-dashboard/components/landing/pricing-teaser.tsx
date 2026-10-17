"use client"

import Link from "next/link"
import { useQuery } from "@tanstack/react-query"
import { Check } from "lucide-react"
import { plansApi } from "@/lib/api/plans"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { cn } from "@/lib/utils"

export function PricingTeaser() {
  const { data: plans, isLoading } = useQuery({ queryKey: ["plans"], queryFn: plansApi.list })

  if (isLoading || !plans) {
    return (
      <div className="grid gap-4 md:grid-cols-3">
        {[0, 1, 2].map((i) => (
          <Skeleton key={i} className="h-72 rounded-xl" />
        ))}
      </div>
    )
  }

  return (
    <div className="grid gap-4 md:grid-cols-3">
      {plans.map((plan) => (
        <div
          key={plan.id}
          className={cn(
            "flex flex-col rounded-xl border bg-card p-6",
            plan.highlighted ? "border-primary ring-1 ring-primary" : "border-border",
          )}
        >
          {plan.highlighted && (
            <span className="mb-3 w-fit rounded-full bg-primary px-2.5 py-0.5 text-xs font-medium text-primary-foreground">
              Más popular
            </span>
          )}
          <h3 className="text-lg font-semibold">{plan.name}</h3>
          <p className="mt-1 text-sm text-muted-foreground">{plan.description}</p>
          <div className="mt-4 flex items-baseline gap-1">
            <span className="text-3xl font-semibold tracking-tight">
              {plan.price === 0 ? "Gratis" : `$${plan.price}`}
            </span>
            {plan.price > 0 && (
              <span className="text-sm text-muted-foreground">/{plan.interval === "month" ? "mes" : "año"}</span>
            )}
          </div>
          <ul className="mt-6 flex flex-1 flex-col gap-2">
            {plan.features.map((f) => (
              <li key={f} className="flex items-start gap-2 text-sm">
                <Check className="mt-0.5 size-4 shrink-0 text-primary" />
                <span className="text-muted-foreground">{f}</span>
              </li>
            ))}
          </ul>
          <Button
            className="mt-6"
            variant={plan.highlighted ? "default" : "outline"}
            render={<Link href="/register" />}
          >
            Comenzar
          </Button>
        </div>
      ))}
    </div>
  )
}
