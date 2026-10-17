"use client"

import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"
import { plansApi } from "@/lib/api/plans"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Badge } from "@/components/ui/badge"
import { Skeleton } from "@/components/ui/skeleton"
import { ArrowUpDown } from "lucide-react"

export default function PlansPage() {
  const qc = useQueryClient()

  const { data, isLoading } = useQuery({
    queryKey: ["admin", "plans"],
    queryFn: plansApi.list,
  })

  const deactivateMutation = useMutation({
    mutationFn: (id: string) => plansApi.deactivate(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["admin", "plans"] })
      toast.success("Plan desactivado")
    },
    onError: (e) => toast.error(e instanceof Error ? e.message : "Error"),
  })

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">Planes</h1>
        <p className="mt-1 text-sm text-muted-foreground">Gestiona los planes de suscripción.</p>
      </div>

      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
        {isLoading ? (
          Array.from({ length: 3 }).map((_, i) => (
            <Card key={i}>
              <CardContent className="p-6"><Skeleton className="h-40 w-full" /></CardContent>
            </Card>
          ))
        ) : data?.length === 0 ? (
          <p className="col-span-full text-center text-sm text-muted-foreground">No hay planes.</p>
        ) : (
          data?.map((plan) => (
            <Card key={plan.id} className={plan.highlighted ? "border-primary/50" : ""}>
              <CardHeader>
                <div className="flex items-center justify-between">
                  <CardTitle className="text-lg">{plan.name}</CardTitle>
                  {plan.highlighted && <Badge>Destacado</Badge>}
                </div>
                <p className="text-sm text-muted-foreground">{plan.description}</p>
              </CardHeader>
              <CardContent className="space-y-4">
                <div>
                  <span className="text-3xl font-bold">{plan.currency ?? "USD"} {plan.price_amount ?? plan.price ?? "0"}</span>
                  <span className="text-muted-foreground">/mes</span>
                </div>
                {plan.features.length > 0 && (
                  <ul className="space-y-1 text-sm text-muted-foreground">
                    {plan.features.map((f, i) => (
                      <li key={i} className="flex items-center gap-2">
                        <ArrowUpDown className="size-3 shrink-0 text-primary" />
                        {f}
                      </li>
                    ))}
                  </ul>
                )}
                <Badge variant={plan.is_active ? "secondary" : "outline"}>
                  {plan.is_active ? "Activo" : "Inactivo"}
                </Badge>
                {plan.is_active && (
                  <Button
                    variant="outline"
                    size="sm"
                    className="w-full"
                    onClick={() => deactivateMutation.mutate(plan.id)}
                    disabled={deactivateMutation.isPending}
                  >
                    Desactivar plan
                  </Button>
                )}
              </CardContent>
            </Card>
          ))
        )}
      </div>
    </div>
  )
}
