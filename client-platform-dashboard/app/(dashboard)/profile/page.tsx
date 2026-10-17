"use client"

import { useState } from "react"
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"
import {
  User,
  KeyRound,
  ShieldCheck,
  ShieldOff,
  Smartphone,
  QrCode,
  Monitor,
  LogOut,
} from "lucide-react"
import { authApi } from "@/lib/api/auth"
import { ApiClientError } from "@/lib/api/client"
import { useSession } from "@/lib/store/session"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Badge } from "@/components/ui/badge"
import { Skeleton } from "@/components/ui/skeleton"
import { profileSchema, changePasswordSchema } from "@/lib/validation"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog"
import type { EnableMfaResponse } from "@/lib/types"

function formatSessionDate(value: string): string {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value
  return date.toLocaleString("es-CO", {
    dateStyle: "medium",
    timeStyle: "short",
  })
}

export default function ProfilePage() {
  const qc = useQueryClient()
  const { user, setUser } = useSession()

  const sessionsQuery = useQuery({
    queryKey: ["auth-sessions"],
    queryFn: authApi.listSessions,
  })

  const revokeSessionMut = useMutation({
    mutationFn: authApi.revokeSession,
    onSuccess: () => {
      toast.success("Sesión revocada")
      qc.invalidateQueries({ queryKey: ["auth-sessions"] })
    },
  })

  // Profile form
  const [displayName, setDisplayName] = useState(user?.display_name ?? "")
  const [profileError, setProfileError] = useState("")

  // Password form
  const [oldPw, setOldPw] = useState("")
  const [newPw, setNewPw] = useState("")
  const [confirmPw, setConfirmPw] = useState("")
  const [pwError, setPwError] = useState("")

  // MFA
  const [mfaOpen, setMfaOpen] = useState(false)
  const [mfaStep, setMfaStep] = useState<"start" | "confirm" | "disable">("start")
  const [mfaCode, setMfaCode] = useState("")
  const [mfaSecret, setMfaSecret] = useState<EnableMfaResponse | null>(null)
  const [mfaError, setMfaError] = useState("")

  const profileMut = useMutation({
    mutationFn: () => {
      const parsed = profileSchema.safeParse({ display_name: displayName })
      if (!parsed.success) {
        setProfileError(parsed.error.issues[0].message)
        throw new Error("validation")
      }
      setProfileError("")
      return authApi.updateProfile({ display_name: displayName || undefined })
    },
    onSuccess: (data) => {
      setUser(data)
      toast.success("Perfil actualizado")
      qc.invalidateQueries({ queryKey: ["me"] })
    },
    onError: (e) => {
      if (e instanceof Error && e.message === "validation") return
      setProfileError(e instanceof ApiClientError ? e.message : "Error al actualizar")
    },
  })

  const passwordMut = useMutation({
    mutationFn: () => {
      const parsed = changePasswordSchema.safeParse({
        old_password: oldPw,
        new_password: newPw,
        confirm_password: confirmPw,
      })
      if (!parsed.success) {
        setPwError(parsed.error.issues[0].message)
        throw new Error("validation")
      }
      setPwError("")
      return authApi.changePassword({ old_password: oldPw, new_password: newPw })
    },
    onSuccess: () => {
      toast.success("Contraseña cambiada")
      setOldPw("")
      setNewPw("")
      setConfirmPw("")
    },
    onError: (e) => {
      if (e instanceof Error && e.message === "validation") return
      setPwError(e instanceof ApiClientError ? e.message : "Error al cambiar contraseña")
    },
  })

  const enableMfaMut = useMutation({
    mutationFn: authApi.enableMfa,
    onSuccess: (data) => {
      setMfaSecret(data)
      setMfaStep("confirm")
    },
    onError: (e) => {
      setMfaError(e instanceof ApiClientError ? e.message : "Error al habilitar MFA")
    },
  })

  const confirmMfaMut = useMutation({
    mutationFn: () => authApi.confirmMfa(mfaCode),
    onSuccess: () => {
      toast.success("MFA habilitado")
      setMfaOpen(false)
      setMfaStep("start")
      setMfaCode("")
      setMfaSecret(null)
      qc.invalidateQueries({ queryKey: ["me"] })
    },
    onError: (e) => {
      setMfaError(e instanceof ApiClientError ? e.message : "Código inválido")
    },
  })

  const disableMfaMut = useMutation({
    mutationFn: () => authApi.disableMfa(mfaCode),
    onSuccess: () => {
      toast.success("MFA deshabilitado")
      setMfaOpen(false)
      setMfaStep("start")
      setMfaCode("")
      setMfaSecret(null)
      qc.invalidateQueries({ queryKey: ["me"] })
    },
    onError: (e) => {
      setMfaError(e instanceof ApiClientError ? e.message : "Código inválido")
    },
  })

  function handleMfaOpen() {
    setMfaStep(user?.mfa_enabled ? "disable" : "start")
    setMfaCode("")
    setMfaError("")
    setMfaSecret(null)
    setMfaOpen(true)
  }

  function handleMfaAction() {
    if (mfaStep === "start") {
      enableMfaMut.mutate()
    } else if (mfaStep === "confirm") {
      confirmMfaMut.mutate()
    } else if (mfaStep === "disable") {
      disableMfaMut.mutate()
    }
  }

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">Perfil</h1>
        <p className="mt-1 text-sm text-muted-foreground">
          Tu información personal y seguridad.
        </p>
      </div>

      {/* Profile info */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-base">
            <User className="size-4" />
            Información personal
          </CardTitle>
          <CardDescription>Tu correo no se puede cambiar. Actualiza tu nombre.</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="flex flex-col gap-2">
            <Label>Correo electrónico</Label>
            <Input value={user?.email ?? ""} disabled />
          </div>
          <div className="flex flex-col gap-2">
            <Label htmlFor="display-name">Nombre público</Label>
            <Input
              id="display-name"
              placeholder="Tu nombre o alias"
              value={displayName}
              onChange={(e) => {
                setDisplayName(e.target.value)
                setProfileError("")
              }}
            />
            {profileError && <p className="text-sm text-destructive">{profileError}</p>}
          </div>
          <Button
            onClick={() => profileMut.mutate()}
            disabled={profileMut.isPending}
          >
            {profileMut.isPending ? "Guardando..." : "Guardar cambios"}
          </Button>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-base">
            <Monitor className="size-4" />
            Sesiones activas
          </CardTitle>
          <CardDescription>
            Revisa dónde está abierta tu cuenta y cierra cualquier sesión que no reconozcas.
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-3">
          {sessionsQuery.isPending && <Skeleton className="h-16 w-full" />}
          {sessionsQuery.isError && (
            <p className="text-sm text-destructive">No se pudieron cargar las sesiones.</p>
          )}
          {sessionsQuery.data?.sessions.length === 0 && (
            <p className="text-sm text-muted-foreground">No hay sesiones activas.</p>
          )}
          {sessionsQuery.data?.sessions.map((session) => (
            <div
              key={session.session_id}
              className="flex flex-col gap-3 rounded-lg border border-border p-4 sm:flex-row sm:items-center sm:justify-between"
            >
              <div className="min-w-0 space-y-1">
                <p className="truncate text-sm font-medium">
                  {session.user_agent || "Navegador no identificado"}
                </p>
                <p className="text-xs text-muted-foreground">
                  {session.ip_address || "IP no disponible"} · Última actividad: {formatSessionDate(session.last_seen_at)}
                </p>
                <p className="text-xs text-muted-foreground">
                  Creada: {formatSessionDate(session.created_at)}
                </p>
              </div>
              <Button
                variant="outline"
                size="sm"
                onClick={() => revokeSessionMut.mutate(session.session_id)}
                disabled={revokeSessionMut.isPending}
              >
                <LogOut className="mr-2 size-3.5" />
                Cerrar sesión
              </Button>
            </div>
          ))}
        </CardContent>
      </Card>

      {/* Change password */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-base">
            <KeyRound className="size-4" />
            Cambiar contraseña
          </CardTitle>
          <CardDescription>Debe tener al menos 12 caracteres, mayúsculas, minúsculas, dígitos y un carácter especial.</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="flex flex-col gap-2">
            <Label htmlFor="old-pw">Contraseña actual</Label>
            <Input
              id="old-pw"
              type="password"
              autoComplete="current-password"
              value={oldPw}
              onChange={(e) => {
                setOldPw(e.target.value)
                setPwError("")
              }}
            />
          </div>
          <div className="flex flex-col gap-2">
            <Label htmlFor="new-pw">Nueva contraseña</Label>
            <Input
              id="new-pw"
              type="password"
              autoComplete="new-password"
              value={newPw}
              onChange={(e) => {
                setNewPw(e.target.value)
                setPwError("")
              }}
            />
          </div>
          <div className="flex flex-col gap-2">
            <Label htmlFor="confirm-pw">Confirmar nueva contraseña</Label>
            <Input
              id="confirm-pw"
              type="password"
              autoComplete="new-password"
              value={confirmPw}
              onChange={(e) => {
                setConfirmPw(e.target.value)
                setPwError("")
              }}
            />
          </div>
          {pwError && <p className="text-sm text-destructive">{pwError}</p>}
          <Button
            onClick={() => passwordMut.mutate()}
            disabled={passwordMut.isPending}
          >
            {passwordMut.isPending ? "Cambiando..." : "Cambiar contraseña"}
          </Button>
        </CardContent>
      </Card>

      {/* MFA */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-base">
            <Smartphone className="size-4" />
            Autenticación multifactor (MFA)
          </CardTitle>
          <CardDescription>
            Añade una capa extra de seguridad a tu cuenta.
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="flex items-center gap-3">
            {user?.mfa_enabled ? (
              <Badge variant="default" className="gap-1">
                <ShieldCheck className="size-3" />
                Habilitado
              </Badge>
            ) : (
              <Badge variant="secondary" className="gap-1">
                <ShieldOff className="size-3" />
                Deshabilitado
              </Badge>
            )}
          </div>
          <Dialog open={mfaOpen} onOpenChange={setMfaOpen}>
            <DialogTrigger render={<Button variant="outline" onClick={handleMfaOpen}>{user?.mfa_enabled ? "Deshabilitar MFA" : "Configurar MFA"}</Button>} />
            <DialogContent>
              <DialogHeader>
                <DialogTitle>
                  {mfaStep === "disable"
                    ? "Deshabilitar MFA"
                    : mfaStep === "confirm"
                      ? "Confirmar MFA"
                      : "Configurar MFA"}
                </DialogTitle>
                <DialogDescription>
                  {mfaStep === "disable"
                    ? "Ingresa tu código MFA para deshabilitarlo."
                    : mfaStep === "confirm"
                      ? "Escanea el código QR con tu app de autenticación (Google Authenticator, Authy, etc.) e ingresa el código de 6 dígitos."
                      : "Se generará un secreto para configurar tu app de autenticación."}
                </DialogDescription>
              </DialogHeader>
              <div className="space-y-4">
                {mfaSecret && (
                  <div className="flex flex-col items-center gap-3 rounded-lg border border-border bg-muted/30 p-4">
                    <QrCode className="size-8 text-muted-foreground" />
                    <pre className="rounded bg-background px-3 py-2 text-xs font-mono select-all">
                      {mfaSecret.secret}
                    </pre>
                    <p className="text-center text-xs text-muted-foreground">
                      O ingresa manualmente el secreto en tu app.
                    </p>
                  </div>
                )}
                <div className="flex flex-col gap-2">
                  <Label htmlFor="mfa-code">Código de 6 dígitos</Label>
                  <Input
                    id="mfa-code"
                    inputMode="numeric"
                    placeholder="123456"
                    value={mfaCode}
                    onChange={(e) => {
                      setMfaCode(e.target.value)
                      setMfaError("")
                    }}
                  />
                  {mfaError && <p className="text-sm text-destructive">{mfaError}</p>}
                </div>
              </div>
              <DialogFooter>
                <Button variant="outline" onClick={() => setMfaOpen(false)}>
                  Cancelar
                </Button>
                <Button
                  onClick={handleMfaAction}
                  disabled={
                    mfaStep === "start"
                      ? enableMfaMut.isPending
                      : mfaStep === "confirm"
                        ? confirmMfaMut.isPending || mfaCode.length !== 6
                        : disableMfaMut.isPending || mfaCode.length !== 6
                  }
                >
                  {mfaStep === "start"
                    ? enableMfaMut.isPending
                      ? "Generando..."
                      : "Generar secreto"
                    : mfaStep === "confirm"
                      ? confirmMfaMut.isPending
                        ? "Confirmando..."
                        : "Confirmar MFA"
                      : disableMfaMut.isPending
                        ? "Deshabilitando..."
                        : "Deshabilitar MFA"}
                </Button>
              </DialogFooter>
            </DialogContent>
          </Dialog>
        </CardContent>
      </Card>
    </div>
  )
}
