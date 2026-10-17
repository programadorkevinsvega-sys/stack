"use client"

import { Suspense, useEffect, useRef, useState } from "react"
import { useRouter, useSearchParams } from "next/navigation"
import Link from "next/link"
import { toast } from "sonner"
import { MailCheck } from "lucide-react"
import { authApi } from "@/lib/api/auth"
import { ApiClientError } from "@/lib/api/client"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"

function VerifyEmailInner() {
  const router = useRouter()
  const params = useSearchParams()
  const email = params.get("email") ?? ""
  const tokenFromUrl = params.get("token") ?? ""
  const [token, setToken] = useState("")
  const [submitting, setSubmitting] = useState(false)
  const autoSubmitted = useRef(false)

  async function verifyToken(value: string) {
    const normalizedToken = value.trim()
    if (!normalizedToken) {
      toast.error("Ingresa el token de verificación")
      return
    }
    setSubmitting(true)
    try {
      await authApi.verifyEmail(normalizedToken)
      toast.success("Correo verificado. Ya puedes iniciar sesión.")
      const loginParams = new URLSearchParams({ redirect: "/plans" })
      if (email) loginParams.set("email", email)
      router.push(`/login?${loginParams.toString()}`)
    } catch (err) {
      toast.error(err instanceof ApiClientError ? err.message : "No se pudo verificar el correo")
    } finally {
      setSubmitting(false)
    }
  }

  useEffect(() => {
    if (!tokenFromUrl || autoSubmitted.current) return
    autoSubmitted.current = true
    setToken(tokenFromUrl)
    void verifyToken(tokenFromUrl)
  }, [tokenFromUrl])

  async function verify(e: React.FormEvent) {
    e.preventDefault()
    await verifyToken(token)
  }

  return (
    <Card>
      <CardHeader>
        <div className="mb-2 flex size-11 items-center justify-center rounded-lg bg-accent text-primary">
          <MailCheck className="size-6" />
        </div>
        <CardTitle className="text-2xl text-balance">Verifica tu correo</CardTitle>
        <CardDescription>
          {email ? (
            <>
              Enviamos un enlace de verificación a <span className="font-medium text-foreground">{email}</span>. Pega el
              token del correo para continuar.
            </>
          ) : (
            "Pega el token de verificación que enviamos a tu correo."
          )}
        </CardDescription>
      </CardHeader>
      <CardContent>
        <form onSubmit={verify} className="flex flex-col gap-4">
          <div className="flex flex-col gap-2">
            <Label htmlFor="token">Token de verificación</Label>
            <Input
              id="token"
              value={token}
              onChange={(e) => setToken(e.target.value)}
              placeholder="Pega tu token aquí"
              autoComplete="one-time-code"
            />
            <p className="text-xs text-muted-foreground">
              Modo demo: escribe cualquier valor (excepto {'"invalid"'}) para verificar.
            </p>
          </div>
          <Button type="submit" className="w-full" disabled={submitting}>
            {submitting ? "Verificando..." : "Verificar correo"}
          </Button>
        </form>
        <p className="mt-6 text-center text-sm text-muted-foreground">
          <Link href="/login" className="font-medium text-primary hover:underline">
            Volver a iniciar sesión
          </Link>
        </p>
      </CardContent>
    </Card>
  )
}

export default function VerifyEmailPage() {
  return (
    <Suspense fallback={null}>
      <VerifyEmailInner />
    </Suspense>
  )
}
