"use client"

import { useState } from "react"
import { useRouter } from "next/navigation"
import { useQuery, useMutation } from "@tanstack/react-query"
import { Check, Loader2 } from "lucide-react"
import { toast } from "sonner"
import type { Plan, PaymentProvider } from "@/lib/types"
import { plansApi } from "@/lib/api/plans"
import { subscriptionApi } from "@/lib/api/subscription"
import { ApiClientError } from "@/lib/api/client"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import { Skeleton } from "@/components/ui/skeleton"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { PaymentProviderSelector } from "@/components/payment-provider-selector"
import { cn } from "@/lib/utils"

function formatPrice(plan: Plan) {
  if (plan.price === 0) return "Gratis"
  return `$${plan.price}`
}

export function PlansGrid() {
  const router = useRouter()
  const [selected, setSelected] = useState<Plan | null>(null)
  const [provider, setProvider] = useState<PaymentProvider | null>(null)
  const [legalAccepted, setLegalAccepted] = useState(false)

  const { data: plans, isLoading, isError, refetch } = useQuery({
    queryKey: ["plans"],
    queryFn: plansApi.list,
  })

  const subscribe = useMutation({
    mutationFn: ({ planId, prov }: { planId: string; prov: PaymentProvider }) =>
      subscriptionApi.acceptLegal().then(() => subscriptionApi.subscribe(planId, prov)),
    onSuccess: (res) => {
      if (res.redirect_url.startsWith("https://")) {
        window.location.assign(res.redirect_url)
        return
      }
      if (res.redirect_url.startsWith("/")) {
        router.push(res.redirect_url)
        return
      }
      toast.error("La pasarela devolvió una redirección inválida")
    },
    onError: (e) => {
      if (e instanceof ApiClientError && e.status === 401) {
        toast.error("Inicia sesión para suscribirte")
        router.push("/login?redirect=/plans")
        return
      }
      toast.error(e instanceof ApiClientError ? e.message : "No se pudo procesar la suscripción")
    },
  })

  function choose(plan: Plan) {
    setSelected(plan)
    setProvider(null)
    setLegalAccepted(false)
  }

  function confirmPaid() {
    if (!selected || !legalAccepted || (selected.price > 0 && !provider)) return
    subscribe.mutate({ planId: selected.id, prov: provider ?? "paypal" })
  }

  if (isLoading) {
    return (
      <div className="grid gap-6 md:grid-cols-3">
        {[0, 1, 2].map((i) => (
          <Skeleton key={i} className="h-96 rounded-xl" />
        ))}
      </div>
    )
  }

  if (isError || !plans) {
    return (
      <div className="flex flex-col items-center gap-4 rounded-xl border border-border bg-card py-16">
        <p className="text-muted-foreground">No se pudieron cargar los planes.</p>
        <Button variant="outline" onClick={() => refetch()}>
          Reintentar
        </Button>
      </div>
    )
  }

  return (
    <>
      <div className="grid gap-6 md:grid-cols-3">
        {plans
          .filter((p) => p.is_active)
          .map((plan) => (
            <div
              key={plan.id}
              className={cn(
                "relative flex flex-col rounded-xl border bg-card p-6",
                plan.highlighted ? "border-primary shadow-lg shadow-primary/10" : "border-border",
              )}
            >
              {plan.highlighted && (
                <Badge className="absolute -top-3 left-6">Más popular</Badge>
              )}
              <h3 className="text-lg font-semibold">{plan.name}</h3>
              <p className="mt-1 text-sm text-muted-foreground">{plan.description}</p>
              <div className="mt-4 flex items-end gap-1">
                <span className="text-4xl font-bold tracking-tight">{formatPrice(plan)}</span>
                {plan.price > 0 && (
                  <span className="pb-1 text-sm text-muted-foreground">
                    /{plan.interval === "month" ? "mes" : "año"}
                  </span>
                )}
              </div>
              <ul className="mt-6 flex flex-1 flex-col gap-3">
                {plan.features.map((f) => (
                  <li key={f} className="flex items-start gap-2 text-sm">
                    <Check className="mt-0.5 size-4 shrink-0 text-primary" />
                    <span>{f}</span>
                  </li>
                ))}
              </ul>
              <Button
                className="mt-6 w-full"
                variant={plan.highlighted ? "default" : "outline"}
                onClick={() => choose(plan)}
                disabled={subscribe.isPending}
              >
                {subscribe.isPending && subscribe.variables?.planId === plan.id && (
                  <Loader2 className="size-4 animate-spin" />
                )}
                {plan.price === 0 ? "Empezar gratis" : `Elegir ${plan.name}`}
              </Button>
            </div>
          ))}
      </div>

      <Dialog open={!!selected} onOpenChange={(open) => !open && setSelected(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Confirmar suscripción</DialogTitle>
            <DialogDescription>
              {selected && (
                <>
                  Plan <span className="font-medium text-foreground">{selected.name}</span> por{" "}
                  <span className="font-medium text-foreground">
                    ${selected.price}/{selected.interval === "month" ? "mes" : "año"}
                  </span>
                  {selected.trial_days && selected.trial_days > 0
                    ? `. Se validará tu cuenta de PayPal sin realizar un cobro y tendrás ${selected.trial_days} días de prueba. Después se cobrará automáticamente si no cancelas.`
                    : ". Elige tu método de pago."}
                </>
              )}
            </DialogDescription>
          </DialogHeader>
          {selected && selected.price > 0 && <PaymentProviderSelector value={provider} onChange={setProvider} />}
          <label className="mt-4 flex items-start gap-2 text-sm">
            <input type="checkbox" checked={legalAccepted} onChange={(e) => setLegalAccepted(e.target.checked)} className="mt-1" />
            <span>Acepto los <a className="underline" href="/terms" target="_blank">términos</a> y la <a className="underline" href="/privacy" target="_blank">política de privacidad</a> (versión 2026-07-24).</span>
          </label>
          <DialogFooter>
            <Button variant="ghost" onClick={() => setSelected(null)}>
              Cancelar
            </Button>
            <Button onClick={confirmPaid} disabled={!legalAccepted || (selected?.price ?? 1) > 0 && !provider || subscribe.isPending}>
              {subscribe.isPending && <Loader2 className="size-4 animate-spin" />}
              Continuar al pago
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  )
}
