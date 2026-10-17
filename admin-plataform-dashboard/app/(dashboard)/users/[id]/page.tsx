"use client"

import { use } from "react"
import { useRouter } from "next/navigation"
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query"
import { ArrowLeft, Ban, LockOpen, CheckCircle, Trash2 } from "lucide-react"
import { toast } from "sonner"
import { usersApi } from "@/lib/api/users"
import { usageApi } from "@/lib/api/usage"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Badge } from "@/components/ui/badge"
import { Skeleton } from "@/components/ui/skeleton"

const statusLabels: Record<string, { label: string; variant: "default" | "secondary" | "outline" | "destructive" }> = {
  active: { label: "Activo", variant: "secondary" },
  disabled: { label: "Deshabilitado", variant: "destructive" },
  locked: { label: "Bloqueado", variant: "outline" },
}

export default function UserDetailPage({ params }: { params: Promise<{ id: string }> }) {
  const { id } = use(params)
  const router = useRouter()
  const qc = useQueryClient()

  const { data: user, isLoading } = useQuery({
    queryKey: ["admin", "users", id],
    queryFn: () => usersApi.get(id),
  })
  const { data: usage } = useQuery({
    queryKey: ["admin", "usage", "users", id],
    queryFn: () => usageApi.byUser(id),
    enabled: Boolean(user),
  })

  const disableMutation = useMutation({
    mutationFn: () => usersApi.disable(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["admin", "users"] })
      toast.success("Usuario deshabilitado")
    },
    onError: (e) => toast.error(e instanceof Error ? e.message : "Error"),
  })

  const unlockMutation = useMutation({
    mutationFn: () => usersApi.unlock(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["admin", "users"] })
      toast.success("Usuario desbloqueado")
    },
    onError: (e) => toast.error(e instanceof Error ? e.message : "Error"),
  })

  const activateMutation = useMutation({
    mutationFn: () => usersApi.activate(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["admin", "users", id] })
      qc.invalidateQueries({ queryKey: ["admin", "users"] })
      toast.success("Usuario activado")
    },
    onError: (e) => toast.error(e instanceof Error ? e.message : "Error"),
  })

  const deleteMutation = useMutation({
    mutationFn: () => usersApi.delete(id),
    onSuccess: () => {
      toast.success("Usuario eliminado y anonimizado")
      router.push("/users")
      qc.invalidateQueries({ queryKey: ["admin", "users"] })
    },
    onError: (e) => toast.error(e instanceof Error ? e.message : "Error"),
  })

  if (isLoading) {
    return (
      <div className="space-y-4">
        <Skeleton className="h-8 w-48" />
        <Skeleton className="h-64 w-full" />
      </div>
    )
  }

  if (!user) {
    return <p className="text-muted-foreground">Usuario no encontrado.</p>
  }

  const st = statusLabels[user.status] ?? { label: user.status, variant: "outline" as const }

  return (
    <div className="space-y-6">
      <div className="flex items-center gap-4">
        <Button variant="ghost" size="icon" onClick={() => router.back()}>
          <ArrowLeft className="size-4" />
        </Button>
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">{user.display_name ?? user.email}</h1>
          <p className="text-sm text-muted-foreground">{user.email}</p>
        </div>
        <Badge variant={st.variant} className="ml-auto">{st.label}</Badge>
      </div>

      <div className="grid gap-4 sm:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle className="text-base">Información</CardTitle>
          </CardHeader>
          <CardContent className="space-y-3 text-sm">
            <div className="flex justify-between">
              <span className="text-muted-foreground">ID</span>
              <span className="font-mono text-xs">{user.id}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-muted-foreground">Email verificado</span>
              <span>{user.email_verified ? "Sí" : "No"}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-muted-foreground">MFA habilitado</span>
              <span>{user.mfa_enabled ? "Sí" : "No"}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-muted-foreground">Rol</span>
              <span>{user.role === "admin" ? "Administrador" : "Usuario"}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-muted-foreground">Creado</span>
              <span>{new Date(user.created_at).toLocaleDateString()}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-muted-foreground">Último login</span>
              <span>{user.last_login_at ? new Date(user.last_login_at).toLocaleDateString() : "—"}</span>
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="text-base">Acciones</CardTitle>
          </CardHeader>
          <CardContent className="space-y-3">
            {user.status === "active" && (
              <Button variant="destructive" className="w-full" onClick={() => disableMutation.mutate()} disabled={disableMutation.isPending}>
                <Ban className="mr-2 size-4" />
                Deshabilitar cuenta
              </Button>
            )}
            {user.status === "locked" && (
              <Button variant="secondary" className="w-full" onClick={() => unlockMutation.mutate()} disabled={unlockMutation.isPending}>
                <LockOpen className="mr-2 size-4" />
                Desbloquear cuenta
              </Button>
            )}
            {user.status === "disabled" && (
              <Button variant="secondary" className="w-full" onClick={() => activateMutation.mutate()} disabled={activateMutation.isPending}>
                <CheckCircle className="mr-2 size-4" />
                Activar cuenta
              </Button>
            )}
            <Button
              variant="destructive"
              className="w-full"
              onClick={() => {
                if (window.confirm("Esta acción anonimiza al usuario y revoca sus sesiones. ¿Continuar?")) {
                  deleteMutation.mutate()
                }
              }}
              disabled={deleteMutation.isPending}
            >
              <Trash2 className="mr-2 size-4" />
              Eliminar y anonimizar
            </Button>
          </CardContent>
        </Card>
      </div>

      <Card>
        <CardHeader><CardTitle className="text-base">Consumo</CardTitle></CardHeader>
        <CardContent className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4 text-sm">
          <div><span className="text-muted-foreground">Requests</span><p className="text-xl font-semibold">{usage?.total_requests.toLocaleString() ?? "—"}</p></div>
          <div><span className="text-muted-foreground">Tokens de entrada</span><p className="text-xl font-semibold">{usage?.total_input_tokens.toLocaleString() ?? "—"}</p></div>
          <div><span className="text-muted-foreground">Tokens de salida</span><p className="text-xl font-semibold">{usage?.total_output_tokens.toLocaleString() ?? "—"}</p></div>
          <div><span className="text-muted-foreground">Errores</span><p className="text-xl font-semibold">{usage ? `${usage.error_count} (${usage.error_rate.toFixed(2)}%)` : "—"}</p></div>
        </CardContent>
      </Card>
    </div>
  )
}
