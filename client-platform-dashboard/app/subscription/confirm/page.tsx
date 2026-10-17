"use client"

import { Suspense, useEffect, useRef } from "react"
import { useSearchParams } from "next/navigation"
import { useRouter } from "next/navigation"
import { useQuery } from "@tanstack/react-query"
import { CheckCircle2, Loader2, XCircle } from "lucide-react"
import { subscriptionApi } from "@/lib/api/subscription"
import { ApiClientError } from "@/lib/api/client"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"

function ConfirmInner() {
  const router = useRouter()
  const params = useSearchParams()
  const confirmed = useRef(false)
  const providerToken = params.get("token") ?? params.get("orderID")
  const planId = params.get("plan_id")
  const isVaultSetup = params.get("flow") === "vault"

  useEffect(() => {
    if (providerToken && planId && !confirmed.current) {
      confirmed.current = true
      void (isVaultSetup
        ? subscriptionApi.confirmPayPalVault(providerToken, planId)
        : subscriptionApi.confirmPayPal(providerToken, planId))
    }
  }, [providerToken, planId, isVaultSetup])

  const { data, isError, error } = useQuery({
    queryKey: ["subscription", "confirm"],
    queryFn: subscriptionApi.get,
    // Poll every 1.5s while the gateway confirms; stop once resolved.
    refetchInterval: (query) => {
      const status = query.state.data?.status
      if (status === "active" || status === "trialing" || status === "past_due" || status === "canceled") {
        return false
      }
      return 1500
    },
    retry: (count, err) => {
      // 404 = subscription record not created yet; keep retrying briefly.
      if (err instanceof ApiClientError && err.status === 404) return count < 5
      return false
    },
  })

  const status = data?.status
  const settled = status === "active" || status === "trialing"

  useEffect(() => {
    if (settled) {
      const t = setTimeout(() => router.push("/dashboard"), 1400)
      return () => clearTimeout(t)
    }
  }, [settled, router])

  return (
    <div className="flex min-h-svh items-center justify-center bg-muted/30 px-4 py-10">
      <Card className="w-full max-w-md text-center">
        <CardHeader>
          <div className="mx-auto mb-2 flex size-14 items-center justify-center rounded-full bg-accent">
            {settled ? (
              <CheckCircle2 className="size-8 text-primary" />
            ) : isError ? (
              <XCircle className="size-8 text-destructive" />
            ) : (
              <Loader2 className="size-8 animate-spin text-primary" />
            )}
          </div>
          <CardTitle className="text-xl">
            {settled
              ? isVaultSetup && status === "trialing"
                ? "¡Prueba activada!"
                : "¡Suscripción activada!"
              : isError
                ? "Algo salió mal"
                : isVaultSetup
                  ? "Validando tu cuenta de PayPal..."
                  : "Confirmando tu pago..."}
          </CardTitle>
          <CardDescription>
            {settled
              ? isVaultSetup && status === "trialing"
                ? `Tu método de pago fue validado y tienes una prueba de 3 días. Redirigiendo a tu panel...`
                : `Tu plan ${data?.plan_name ?? ""} está activo. Redirigiendo a tu panel...`
              : isError
                ? error instanceof ApiClientError
                  ? error.message
                  : "No pudimos confirmar tu suscripción."
                : isVaultSetup
                  ? "PayPal está validando el método de pago. No se realizará ningún cobro durante esta verificación."
                  : "Estamos verificando la confirmación de la pasarela de pago. Esto puede tardar unos segundos."}
          </CardDescription>
        </CardHeader>
        <CardContent>
          {settled ? (
            <Button className="w-full" onClick={() => router.push("/dashboard")}>
              Ir al panel
            </Button>
          ) : isError ? (
            <Button variant="outline" className="w-full" onClick={() => router.push("/plans")}>
              Volver a planes
            </Button>
          ) : (
            <p className="text-xs text-muted-foreground">No cierres esta ventana.</p>
          )}
        </CardContent>
      </Card>
    </div>
  )
}

export default function SubscriptionConfirmPage() {
  return (
    <Suspense fallback={null}>
      <ConfirmInner />
    </Suspense>
  )
}
