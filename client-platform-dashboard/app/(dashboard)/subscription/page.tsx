"use client"

import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query"
import Link from "next/link"
import { toast } from "sonner"
import {
  CreditCard,
  CheckCircle2,
  AlertCircle,
  Loader2,
  Calendar,
  Ban,
  ArrowUpRight,
} from "lucide-react"
import { subscriptionApi } from "@/lib/api/subscription"
import { plansApi } from "@/lib/api/plans"
import { ApiClientError } from "@/lib/api/client"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
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
  DialogTrigger,
} from "@/components/ui/dialog"
import { useState } from "react"

const statusLabel: Record<string, string> = {
  active: "Activa",
  trialing: "Prueba",
  past_due: "Vencida",
  canceled: "Cancelada",
  incomplete: "Incompleta",
  processing: "Procesando",
}

const statusVariant: Record<string, "default" | "secondary" | "outline" | "destructive"> = {
  active: "default",
  trialing: "default",
  past_due: "destructive",
  canceled: "secondary",
  incomplete: "outline",
  processing: "outline",
}

export default function SubscriptionPage() {
  const qc = useQueryClient()
  const [cancelOpen, setCancelOpen] = useState(false)

  const sub = useQuery({
    queryKey: ["subscription"],
    queryFn: subscriptionApi.get,
    retry: false,
  })
  const plans = useQuery({
    queryKey: ["plans"],
    queryFn: plansApi.list,
  })

  const cancelMut = useMutation({
    mutationFn: subscriptionApi.cancel,
    onSuccess: () => {
      toast.success("Suscripción cancelada. Seguirá activa hasta el final del período.")
      setCancelOpen(false)
      qc.invalidateQueries({ queryKey: ["subscription"] })
    },
    onError: (e) => {
      toast.error(e instanceof ApiClientError ? e.message : "Error al cancelar")
    },
  })

  const isSubLoading = sub.isLoading
  const is404 = sub.error instanceof ApiClientError && sub.error.status === 404

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">Suscripción</h1>
        <p className="mt-1 text-sm text-muted-foreground">
          Gestiona tu plan y ciclo de facturación.
        </p>
      </div>

      {isSubLoading ? (
        <Card>
          <CardHeader><Skeleton className="h-5 w-32" /></CardHeader>
          <CardContent className="space-y-3">
            <Skeleton className="h-4 w-48" />
            <Skeleton className="h-4 w-36" />
            <Skeleton className="h-4 w-40" />
          </CardContent>
        </Card>
      ) : is404 ? (
        <Card>
          <CardHeader>
            <CardTitle>Sin suscripción activa</CardTitle>
            <CardDescription>
              No tienes un plan activo. Elige un plan para empezar a usar la plataforma.
            </CardDescription>
          </CardHeader>
          <CardContent>
            <Button render={<Link href="/plans" />}>
              Ver planes
              <ArrowUpRight className="ml-2 size-4" />
            </Button>
          </CardContent>
        </Card>
      ) : sub.data ? (
        <>
          <Card>
            <CardHeader className="flex flex-row items-start justify-between">
              <div>
                <CardTitle className="text-xl">{sub.data.plan_name}</CardTitle>
                <CardDescription>
                  <Badge variant={statusVariant[sub.data.status] ?? "outline"} className="mt-1">
                    {statusLabel[sub.data.status] ?? sub.data.status}
                  </Badge>
                </CardDescription>
              </div>
              <CreditCard className="size-8 text-muted-foreground" />
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="grid gap-3 sm:grid-cols-2">
                <div className="rounded-lg border border-border bg-muted/30 p-3">
                  <div className="flex items-center gap-2 text-sm text-muted-foreground">
                    <Calendar className="size-4" />
                    Inicio del período
                  </div>
                  <p className="mt-1 font-medium">
                    {new Date(sub.data.current_period_start).toLocaleDateString()}
                  </p>
                </div>
                <div className="rounded-lg border border-border bg-muted/30 p-3">
                  <div className="flex items-center gap-2 text-sm text-muted-foreground">
                    <Calendar className="size-4" />
                    Fin del período
                  </div>
                  <p className="mt-1 font-medium">
                    {new Date(sub.data.current_period_end).toLocaleDateString()}
                  </p>
                </div>
              </div>

              <div className="flex items-center gap-2 text-sm">
                {sub.data.cancel_at_period_end ? (
                  <span className="flex items-center gap-1 text-destructive">
                    <AlertCircle className="size-4" />
                    Se cancelará al final del período actual
                  </span>
                ) : (
                  <span className="flex items-center gap-1 text-primary">
                    <CheckCircle2 className="size-4" />
                    Renovación automática
                  </span>
                )}
              </div>
            </CardContent>
          </Card>

          <div className="flex flex-wrap gap-3">
            <Button variant="outline" render={<Link href="/plans" />}>
              Cambiar plan
              <ArrowUpRight className="ml-2 size-4" />
            </Button>

            {!sub.data.cancel_at_period_end && (
              <Dialog open={cancelOpen} onOpenChange={setCancelOpen}>
                <DialogTrigger render={<Button variant="outline" className="text-destructive"><Ban className="mr-2 size-4" />Cancelar suscripción</Button>} />
                <DialogContent>
                  <DialogHeader>
                    <DialogTitle>Cancelar suscripción</DialogTitle>
                    <DialogDescription>
                      Tu plan seguirá activo hasta el final del período actual ({new Date(sub.data.current_period_end).toLocaleDateString()}). Después no se renovará.
                    </DialogDescription>
                  </DialogHeader>
                  <DialogFooter>
                    <Button variant="outline" onClick={() => setCancelOpen(false)}>
                      Mantener
                    </Button>
                    <Button
                      variant="destructive"
                      onClick={() => cancelMut.mutate()}
                      disabled={cancelMut.isPending}
                    >
                      {cancelMut.isPending ? "Cancelando..." : "Sí, cancelar"}
                    </Button>
                  </DialogFooter>
                </DialogContent>
              </Dialog>
            )}
          </div>
        </>
      ) : (
        <Card>
          <CardHeader>
            <CardTitle>Error al cargar</CardTitle>
            <CardDescription>
              {sub.error instanceof ApiClientError
                ? sub.error.message
                : "No se pudo obtener la información de la suscripción."}
            </CardDescription>
          </CardHeader>
          <CardContent>
            <Button variant="outline" onClick={() => sub.refetch()}>
              Reintentar
            </Button>
          </CardContent>
        </Card>
      )}
    </div>
  )
}
