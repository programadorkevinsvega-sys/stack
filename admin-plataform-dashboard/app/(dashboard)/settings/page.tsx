"use client"

import { useMutation } from "@tanstack/react-query"
import { toast } from "sonner"
import { RefreshCw, Database } from "lucide-react"
import { usageApi } from "@/lib/api/usage"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Button } from "@/components/ui/button"

export default function SettingsPage() {
  const refreshMutation = useMutation({
    mutationFn: () => usageApi.refreshViews(),
    onSuccess: () => toast.success("Vistas materializadas refrescadas"),
    onError: (e) => toast.error(e instanceof Error ? e.message : "Error"),
  })

  const partitionMutation = useMutation({
    mutationFn: () => usageApi.createPartition(),
    onSuccess: () => toast.success("Partición creada"),
    onError: (e) => toast.error(e instanceof Error ? e.message : "Error"),
  })

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">Configuración</h1>
        <p className="mt-1 text-sm text-muted-foreground">Acciones de mantenimiento del sistema.</p>
      </div>

      <div className="grid gap-4 sm:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2 text-base">
              <RefreshCw className="size-4" />
              Vistas materializadas
            </CardTitle>
            <CardDescription>
              Refresca las vistas de uso para que reflejen los datos más recientes.
            </CardDescription>
          </CardHeader>
          <CardContent>
            <Button onClick={() => refreshMutation.mutate()} disabled={refreshMutation.isPending}>
              {refreshMutation.isPending ? "Refrescando..." : "Refrescar vistas"}
            </Button>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2 text-base">
              <Database className="size-4" />
              Partición de uso
            </CardTitle>
            <CardDescription>
              Crea la partición del próximo mes para la tabla de uso.
            </CardDescription>
          </CardHeader>
          <CardContent>
            <Button onClick={() => partitionMutation.mutate()} disabled={partitionMutation.isPending}>
              {partitionMutation.isPending ? "Creando..." : "Crear partición"}
            </Button>
          </CardContent>
        </Card>
      </div>
    </div>
  )
}
