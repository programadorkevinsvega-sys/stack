"use client"

import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"
import { CreditCard, CheckCircle2, Star } from "lucide-react"
import { paymentMethodsApi } from "@/lib/api/paymentMethods"
import { ApiClientError } from "@/lib/api/client"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import { Skeleton } from "@/components/ui/skeleton"

const providerLabel: Record<string, string> = {
  card: "Tarjeta",
  stripe: "Stripe",
  paypal: "PayPal",
  payoneer: "Payoneer",
  wompi: "Wompi",
  payu: "PayU",
}

export default function PaymentMethodsPage() {
  const qc = useQueryClient()

  const { data: methods, isLoading } = useQuery({
    queryKey: ["payment-methods"],
    queryFn: paymentMethodsApi.list,
  })

  const defaultMut = useMutation({
    mutationFn: (id: string) => paymentMethodsApi.setDefault(id),
    onSuccess: () => {
      toast.success("Método de pago predeterminado actualizado")
      qc.invalidateQueries({ queryKey: ["payment-methods"] })
    },
    onError: (e) => {
      toast.error(e instanceof ApiClientError ? e.message : "Error al actualizar")
    },
  })

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">Métodos de pago</h1>
        <p className="mt-1 text-sm text-muted-foreground">
          Administra tus formas de pago registradas.
        </p>
      </div>

      {isLoading ? (
        <div className="space-y-3">
          {Array.from({ length: 2 }).map((_, i) => (
            <Card key={i}>
              <CardContent className="p-4">
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-3">
                    <Skeleton className="size-10 rounded-lg" />
                    <div>
                      <Skeleton className="h-4 w-28" />
                      <Skeleton className="mt-1 h-3 w-20" />
                    </div>
                  </div>
                  <Skeleton className="h-8 w-20" />
                </div>
              </CardContent>
            </Card>
          ))}
        </div>
      ) : methods?.length ? (
        <div className="space-y-3">
          {methods.map((method) => (
            <Card key={method.id}>
              <CardContent className="flex items-center justify-between p-4">
                <div className="flex items-center gap-3">
                  <div className="flex size-10 items-center justify-center rounded-lg bg-accent">
                    <CreditCard className="size-5 text-primary" />
                  </div>
                  <div>
                    <div className="flex items-center gap-2">
                      <p className="font-medium">
                        {providerLabel[method.type] ?? method.type}
                      </p>
                      {method.is_default && (
                        <Badge variant="secondary" className="gap-1 text-[10px]">
                          <Star className="size-3" />
                          Default
                        </Badge>
                      )}
                    </div>
                    <p className="text-sm text-muted-foreground">
                      {method.last_four
                        ? `**** ${method.last_four}`
                        : "Vinculado"}
                      {method.expiry_month && method.expiry_year
                        ? ` — ${String(method.expiry_month).padStart(2, "0")}/${method.expiry_year}`
                        : ""}
                    </p>
                  </div>
                </div>
                {!method.is_default && (
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => defaultMut.mutate(method.id)}
                    disabled={defaultMut.isPending}
                  >
                    <CheckCircle2 className="mr-2 size-4" />
                    Hacer default
                  </Button>
                )}
              </CardContent>
            </Card>
          ))}
        </div>
      ) : (
        <Card>
          <CardHeader>
            <CardTitle>Sin métodos de pago</CardTitle>
            <CardDescription>
              No has registrado ningún método de pago. Para recargar tu billetera, serás redirigido a la pasarela de pago.
            </CardDescription>
          </CardHeader>
        </Card>
      )}
    </div>
  )
}
