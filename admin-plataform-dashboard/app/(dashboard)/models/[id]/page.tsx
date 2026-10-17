"use client"

import { use } from "react"
import { useRouter } from "next/navigation"
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query"
import { useForm } from "react-hook-form"
import { zodResolver } from "@hookform/resolvers/zod"
import { toast } from "sonner"
import { z } from "zod"
import { ArrowLeft, KeyRound } from "lucide-react"
import { modelsApi } from "@/lib/api/models"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Skeleton } from "@/components/ui/skeleton"

const modelEditSchema = z.object({
  display_name: z.string().min(1, "Obligatorio"),
  pricing_input: z.number().min(0, "Debe ser >= 0"),
  pricing_output: z.number().min(0, "Debe ser >= 0"),
  context_window: z.number().int().positive().optional(),
  max_output_tokens: z.number().int().positive().optional(),
  description: z.string().optional(),
  provider_model_id: z.string().min(1, "Obligatorio"),
})
type ModelEditValues = z.infer<typeof modelEditSchema>

export default function ModelDetailPage({ params }: { params: Promise<{ id: string }> }) {
  const { id } = use(params)
  const router = useRouter()
  const qc = useQueryClient()

  const { data: model, isLoading } = useQuery({
    queryKey: ["admin", "models", id],
    queryFn: () => modelsApi.get(id),
  })
  const { data: accounts } = useQuery({
    queryKey: ["admin", "aws-accounts"],
    queryFn: modelsApi.listAccounts,
  })

  const { register, handleSubmit, formState: { errors } } = useForm<ModelEditValues>({
    resolver: zodResolver(modelEditSchema),
    values: model ? {
      display_name: model.display_name,
      pricing_input: model.pricing_input,
      pricing_output: model.pricing_output,
      context_window: model.context_window,
      max_output_tokens: model.max_output_tokens,
      description: model.description ?? "",
      provider_model_id: model.provider_model_id ?? "",
    } : undefined,
  })

  const updateMutation = useMutation({
    mutationFn: (values: ModelEditValues) => modelsApi.update(id, values),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["admin", "models"] })
      toast.success("Modelo actualizado")
    },
    onError: (e) => toast.error(e instanceof Error ? e.message : "Error"),
  })

  const deprecateMutation = useMutation({
    mutationFn: () => modelsApi.deprecate(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["admin", "models"] })
      toast.success("Modelo desactivado")
    },
    onError: (e) => toast.error(e instanceof Error ? e.message : "Error"),
  })

  const mantleForm = useForm({
    defaultValues: { account_id: "", api_key: "", base_url: "", expires_at: "" },
  })
  const mantleMutation = useMutation({
    mutationFn: (values: { account_id: string; api_key: string; base_url: string; expires_at: string }) =>
      modelsApi.upsertMantleConnection(values.account_id, id, {
        api_key: values.api_key,
        base_url: values.base_url,
        expires_at: values.expires_at || undefined,
      }),
    onSuccess: () => {
      mantleForm.reset({ account_id: "", api_key: "", base_url: "", expires_at: "" })
      toast.success("Credencial Mantle actualizada")
    },
    onError: (e) => toast.error(e instanceof Error ? e.message : "Error al actualizar la credencial"),
  })

  if (isLoading) return <Skeleton className="h-64 w-full" />
  if (!model) return <p className="text-muted-foreground">Modelo no encontrado.</p>

  return (
    <div className="space-y-6">
      <div className="flex items-center gap-4">
        <Button variant="ghost" size="icon" onClick={() => router.back()}>
          <ArrowLeft className="size-4" />
        </Button>
        <h1 className="text-2xl font-semibold tracking-tight">{model.display_name}</h1>
      </div>

      <form onSubmit={handleSubmit((values) => updateMutation.mutate(values))} className="grid gap-4 sm:grid-cols-2">
        <Card className="sm:col-span-2">
          <CardHeader><CardTitle className="text-base">Información</CardTitle></CardHeader>
          <CardContent className="grid gap-4 sm:grid-cols-2">
            <div className="flex flex-col gap-2">
              <Label htmlFor="display_name">Nombre</Label>
              <Input id="display_name" {...register("display_name")} aria-invalid={!!errors.display_name} />
            </div>
            <div className="flex flex-col gap-2">
              <Label>Proveedor</Label>
              <Input value={model.provider} disabled />
            </div>
            <div className="flex flex-col gap-2">
              <Label>Familia</Label>
              <Input value={model.family} disabled />
            </div>
            <div className="flex flex-col gap-2">
              <Label htmlFor="provider_model_id">ID del modelo en el proveedor</Label>
              <Input id="provider_model_id" {...register("provider_model_id")} aria-invalid={!!errors.provider_model_id} />
            </div>
            <div className="flex flex-col gap-2">
              <Label htmlFor="description">Descripción</Label>
              <Input id="description" {...register("description")} />
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader><CardTitle className="text-base">Precios</CardTitle></CardHeader>
          <CardContent className="space-y-4">
            <div className="flex flex-col gap-2">
              <Label htmlFor="pricing_input">Precio input (por token)</Label>
              <Input id="pricing_input" type="number" step="0.0001" {...register("pricing_input", { valueAsNumber: true })} />
            </div>
            <div className="flex flex-col gap-2">
              <Label htmlFor="pricing_output">Precio output (por token)</Label>
              <Input id="pricing_output" type="number" step="0.0001" {...register("pricing_output", { valueAsNumber: true })} />
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader><CardTitle className="text-base">Límites</CardTitle></CardHeader>
          <CardContent className="space-y-4">
            <div className="flex flex-col gap-2">
              <Label htmlFor="context_window">Ventana de contexto</Label>
              <Input id="context_window" type="number" {...register("context_window", { valueAsNumber: true })} />
            </div>
            <div className="flex flex-col gap-2">
              <Label htmlFor="max_output_tokens">Máx. tokens output</Label>
              <Input id="max_output_tokens" type="number" {...register("max_output_tokens", { valueAsNumber: true })} />
            </div>
          </CardContent>
        </Card>

        <div className="sm:col-span-2 flex gap-3">
          <Button type="submit" disabled={updateMutation.isPending}>
            Guardar cambios
          </Button>
          {model.is_active && (
            <Button variant="destructive" onClick={() => deprecateMutation.mutate()} disabled={deprecateMutation.isPending}>
              Desactivar modelo
            </Button>
          )}
        </div>
      </form>

      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-base"><KeyRound className="size-4" />Conexión Bedrock Mantle por cuenta</CardTitle>
          <p className="text-sm text-muted-foreground">La API key se cifra en el backend y nunca se muestra después de guardarla.</p>
        </CardHeader>
        <CardContent>
          <form onSubmit={mantleForm.handleSubmit((values) => mantleMutation.mutate(values))} className="grid gap-4 sm:grid-cols-2">
            <div className="flex flex-col gap-2">
              <Label htmlFor="account_id">Cuenta AWS</Label>
              <select id="account_id" className="h-10 rounded-md border border-input bg-background px-3 text-sm" {...mantleForm.register("account_id", { required: true })}>
                <option value="">Selecciona una cuenta</option>
                {accounts?.filter((account) => account.is_active).map((account) => (
                  <option key={account.account_id} value={account.account_id}>
                    {account.account_alias} · {account.aws_account_id} · {account.region}
                  </option>
                ))}
              </select>
            </div>
            <div className="flex flex-col gap-2">
              <Label htmlFor="base_url">URL base de Mantle</Label>
              <Input id="base_url" type="url" placeholder="https://..." {...mantleForm.register("base_url", { required: true })} />
            </div>
            <div className="flex flex-col gap-2 sm:col-span-2">
              <Label htmlFor="api_key">API key de Mantle</Label>
              <Input id="api_key" type="password" autoComplete="new-password" {...mantleForm.register("api_key", { required: true })} />
            </div>
            <div className="flex flex-col gap-2">
              <Label htmlFor="expires_at">Expira (RFC3339, opcional)</Label>
              <Input id="expires_at" placeholder="2026-07-26T00:00:00Z" {...mantleForm.register("expires_at")} />
            </div>
            <div className="sm:col-span-2">
              <Button type="submit" disabled={mantleMutation.isPending}>Guardar credencial Mantle</Button>
            </div>
          </form>
        </CardContent>
      </Card>
    </div>
  )
}
