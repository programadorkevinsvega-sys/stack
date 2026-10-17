"use client"

import { Suspense, useState } from "react"
import { useRouter, useSearchParams } from "next/navigation"
import Link from "next/link"
import { useForm } from "react-hook-form"
import { zodResolver } from "@hookform/resolvers/zod"
import { toast } from "sonner"
import { loginSchema, type LoginValues } from "@/lib/validation"
import { authApi } from "@/lib/api/auth"
import { ApiClientError } from "@/lib/api/client"
import { useSession } from "@/lib/store/session"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"

function LoginInner() {
  const router = useRouter()
  const params = useSearchParams()
  const redirect = params.get("redirect") || "/dashboard"
  const setSession = useSession((s) => s.setSession)
  const [submitting, setSubmitting] = useState(false)
  const [mfaRequired, setMfaRequired] = useState(false)

  const {
    register,
    handleSubmit,
    setError,
    formState: { errors },
  } = useForm<LoginValues>({
    resolver: zodResolver(loginSchema),
    defaultValues: { email: "" },
  })

  async function onSubmit(values: LoginValues) {
    setSubmitting(true)
    try {
      const res = await authApi.login({
        email: values.email,
        password: values.password,
        mfa_code: values.mfa_code || undefined,
      })
      setSession(res.user, res.refresh_token ?? null)
      toast.success("Bienvenido")
      router.push(redirect)
    } catch (e) {
      if (e instanceof ApiClientError) {
        if (e.fields) {
          for (const [field, message] of Object.entries(e.fields)) {
            setError(field as keyof LoginValues, { message })
          }
        }
        if (e.status === 401 && /mfa/i.test(e.message)) {
          setMfaRequired(true)
          toast.info("Ingresa tu código MFA para continuar")
        } else {
          toast.error(e.message)
        }
      } else {
        toast.error("No se pudo iniciar sesión")
      }
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-2xl text-balance">Admin Nexus</CardTitle>
        <CardDescription>Accede al panel de administración.</CardDescription>
      </CardHeader>
      <CardContent>
        <form onSubmit={handleSubmit(onSubmit)} className="flex flex-col gap-4" noValidate>
          <div className="flex flex-col gap-2">
            <Label htmlFor="email">Correo</Label>
            <Input id="email" type="email" autoComplete="email" placeholder="admin@nexus.ai" aria-invalid={!!errors.email} aria-describedby={errors.email ? "email-error" : undefined} {...register("email")} />
            {errors.email && <p id="email-error" className="text-sm text-destructive">{errors.email.message}</p>}
          </div>
          <div className="flex flex-col gap-2">
            <Label htmlFor="password">Contraseña</Label>
            <Input id="password" type="password" autoComplete="current-password" aria-invalid={!!errors.password} aria-describedby={errors.password ? "password-error" : undefined} {...register("password")} />
            {errors.password && <p id="password-error" className="text-sm text-destructive">{errors.password.message}</p>}
          </div>
          {mfaRequired && (
            <div className="flex flex-col gap-2">
              <Label htmlFor="mfa_code">Código MFA</Label>
              <Input id="mfa_code" inputMode="numeric" autoComplete="one-time-code" placeholder="123456" aria-invalid={!!errors.mfa_code} aria-describedby={errors.mfa_code ? "mfa_code-error" : undefined} {...register("mfa_code")} />
              {errors.mfa_code && <p id="mfa_code-error" className="text-sm text-destructive">{errors.mfa_code.message}</p>}
            </div>
          )}
          <Button type="submit" className="mt-2 w-full" disabled={submitting}>
            {submitting ? "Iniciando sesión..." : "Iniciar sesión"}
          </Button>
        </form>
      </CardContent>
    </Card>
  )
}

export default function LoginPage() {
  return (
    <Suspense fallback={null}>
      <LoginInner />
    </Suspense>
  )
}
