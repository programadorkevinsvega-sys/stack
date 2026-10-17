"use client"

import { useState } from "react"
import { useRouter } from "next/navigation"
import Link from "next/link"
import { useForm } from "react-hook-form"
import { zodResolver } from "@hookform/resolvers/zod"
import { toast } from "sonner"
import { registerSchema, type RegisterValues } from "@/lib/validation"
import { authApi } from "@/lib/api/auth"
import { ApiClientError } from "@/lib/api/client"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"

export default function RegisterPage() {
  const router = useRouter()
  const [submitting, setSubmitting] = useState(false)
  const {
    register,
    handleSubmit,
    setError,
    formState: { errors },
  } = useForm<RegisterValues>({ resolver: zodResolver(registerSchema) })

  async function onSubmit(values: RegisterValues) {
    setSubmitting(true)
    try {
      await authApi.register({
        email: values.email,
        password: values.password,
        display_name: values.display_name || undefined,
      })
      toast.success("Cuenta creada. Revisa tu correo para verificarla.")
      router.push(`/verify-email?email=${encodeURIComponent(values.email)}`)
    } catch (e) {
      if (e instanceof ApiClientError) {
        if (e.fields) {
          for (const [field, message] of Object.entries(e.fields)) {
            setError(field as keyof RegisterValues, { message })
          }
        }
        toast.error(e.message)
      } else {
        toast.error("Algo salió mal. Inténtalo de nuevo.")
      }
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-2xl text-balance">Crea tu cuenta</CardTitle>
        <CardDescription>Empieza a construir con la API de inferencia en minutos.</CardDescription>
      </CardHeader>
      <CardContent>
        <form onSubmit={handleSubmit(onSubmit)} className="flex flex-col gap-4" noValidate>
          <div className="flex flex-col gap-2">
            <Label htmlFor="display_name">Nombre (opcional)</Label>
            <Input id="display_name" autoComplete="name" placeholder="Tu nombre" aria-invalid={!!errors.display_name} aria-describedby={errors.display_name ? "display_name-error" : undefined} {...register("display_name")} />
            {errors.display_name && <p id="display_name-error" className="text-sm text-destructive">{errors.display_name.message}</p>}
          </div>
          <div className="flex flex-col gap-2">
            <Label htmlFor="email">Correo</Label>
            <Input id="email" type="email" autoComplete="email" placeholder="tu@empresa.com" aria-invalid={!!errors.email} aria-describedby={errors.email ? "email-error" : undefined} {...register("email")} />
            {errors.email && <p id="email-error" className="text-sm text-destructive">{errors.email.message}</p>}
          </div>
          <div className="flex flex-col gap-2">
            <Label htmlFor="password">Contraseña</Label>
            <Input id="password" type="password" autoComplete="new-password" aria-invalid={!!errors.password} aria-describedby={errors.password ? "password-error" : undefined} {...register("password")} />
            {errors.password && <p id="password-error" className="text-sm text-destructive">{errors.password.message}</p>}
          </div>
          <div className="flex flex-col gap-2">
            <Label htmlFor="confirm_password">Confirmar contraseña</Label>
            <Input
              id="confirm_password"
              type="password"
              autoComplete="new-password"
              aria-invalid={!!errors.confirm_password}
              aria-describedby={errors.confirm_password ? "confirm_password-error" : undefined}
              {...register("confirm_password")}
            />
            {errors.confirm_password && <p id="confirm_password-error" className="text-sm text-destructive">{errors.confirm_password.message}</p>}
          </div>
          <Button type="submit" className="mt-2 w-full" disabled={submitting}>
            {submitting ? "Creando cuenta..." : "Crear cuenta"}
          </Button>
        </form>
        <p className="mt-6 text-center text-sm text-muted-foreground">
          {"¿Ya tienes cuenta? "}
          <Link href="/login" className="font-medium text-primary hover:underline">
            Inicia sesión
          </Link>
        </p>
      </CardContent>
    </Card>
  )
}
