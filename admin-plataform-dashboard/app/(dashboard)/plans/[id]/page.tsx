"use client"

import { use } from "react"
import { useRouter } from "next/navigation"
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query"
import { useForm } from "react-hook-form"
import { zodResolver } from "@hookform/resolvers/zod"
import { toast } from "sonner"
import { ArrowLeft } from "lucide-react"
import { plansApi } from "@/lib/api/plans"
import { createPlanSchema, type CreatePlanValues } from "@/lib/validation"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Skeleton } from "@/components/ui/skeleton"

export default function PlanDetailPage({ params }: { params: Promise<{ id: string }> }) {
  const { id } = use(params)
  const router = useRouter()
  const qc = useQueryClient()

  const { data: plan, isLoading } = useQuery({
    queryKey: ["admin", "plans", id],
    queryFn: () => plansApi.get(id),
  })

  const { register, handleSubmit, setValue, formState: { errors } } = useForm<CreatePlanValues>({
    resolver: zodResolver(createPlanSchema),
    values: plan ? {
      name: plan.name,
      slug: plan.slug ?? "",
      requests_per_minute: plan.requests_per_minute ?? 1,
      tokens_per_minute: plan.tokens_per_minute ?? 1,
      max_concurrent_requests: plan.max_concurrent_requests ?? 1,
      monthly_token_quota: plan.monthly_token_quota ?? null,
      price_multiplier: plan.price_multiplier ?? "1",
      billing_mode: plan.billing_mode ?? "prepaid",
      storage_bytes_quota: plan.storage_bytes_quota ?? 0,
      max_file_size_bytes: plan.max_file_size_bytes ?? 0,
      max_files: plan.max_files ?? 0,
      embeddings_enabled: plan.embeddings_enabled ?? false,
      rag_enabled: plan.rag_enabled ?? false,
      qdrant_mode: plan.qdrant_mode ?? "none",
      price_amount: plan.price_amount ?? "0",
      currency: plan.currency ?? "USD",
      trial_days: plan.trial_days ?? 0,
      max_api_keys: plan.max_api_keys ?? 0,
      overage_enabled: plan.overage_enabled ?? false,
      fair_use_policy: plan.fair_use_policy ?? "",
      grace_period_days: plan.grace_period_days ?? 0,
      sso_enabled: plan.sso_enabled ?? false,
      sla_enabled: plan.sla_enabled ?? false,
      mtls_enabled: plan.mtls_enabled ?? false,
      s3_api_enabled: plan.s3_api_enabled ?? false,
      features_json: JSON.stringify(plan.features ?? []),
    } : undefined,
  })

  const updateMutation = useMutation({
    mutationFn: (values: CreatePlanValues) => plansApi.update(id, values),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["admin", "plans"] })
      toast.success("Plan actualizado")
    },
    onError: (e) => toast.error(e instanceof Error ? e.message : "Error"),
  })

  const deactivateMutation = useMutation({
    mutationFn: () => plansApi.deactivate(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["admin", "plans"] })
      toast.success("Plan desactivado")
    },
    onError: (e) => toast.error(e instanceof Error ? e.message : "Error"),
  })

  if (isLoading) return <Skeleton className="h-64 w-full" />
  if (!plan) return <p className="text-muted-foreground">Plan no encontrado.</p>

  return (
    <div className="space-y-6">
      <div className="flex items-center gap-4">
        <Button variant="ghost" size="icon" onClick={() => router.back()}>
          <ArrowLeft className="size-4" />
        </Button>
        <h1 className="text-2xl font-semibold tracking-tight">{plan.name}</h1>
      </div>

      <form onSubmit={handleSubmit((values) => updateMutation.mutate(values))} className="space-y-4">
        <Card>
          <CardHeader><CardTitle className="text-base">Detalles del plan</CardTitle></CardHeader>
          <CardContent className="grid gap-4 sm:grid-cols-2">
            <div className="flex flex-col gap-2">
              <Label htmlFor="name">Nombre</Label>
              <Input id="name" {...register("name")} aria-invalid={!!errors.name} />
            </div>
            <div className="flex flex-col gap-2">
              <Label htmlFor="slug">Slug</Label><Input id="slug" {...register("slug")} />
            </div>
            <div className="flex flex-col gap-2">
              <Label htmlFor="price_amount">Precio</Label><Input id="price_amount" {...register("price_amount")} />
            </div>
            <div className="flex flex-col gap-2">
              <Label htmlFor="currency">Moneda</Label><Input id="currency" {...register("currency")} />
            </div>
            {(["requests_per_minute","tokens_per_minute","max_concurrent_requests","storage_bytes_quota","max_file_size_bytes","max_files","trial_days","max_api_keys","grace_period_days"] as const).map((field) => (
              <div className="flex flex-col gap-2" key={field}><Label htmlFor={field}>{field}</Label><Input id={field} type="number" {...register(field, { valueAsNumber: true })} /></div>
            ))}
            <div className="flex flex-col gap-2"><Label htmlFor="qdrant_mode">Qdrant</Label><Select defaultValue={plan.qdrant_mode ?? "none"} onValueChange={(v) => setValue("qdrant_mode", v as "none" | "shared" | "dedicated")}><SelectTrigger><SelectValue /></SelectTrigger><SelectContent><SelectItem value="none">Sin Qdrant</SelectItem><SelectItem value="shared">Compartido</SelectItem><SelectItem value="dedicated">Dedicado</SelectItem></SelectContent></Select></div>
            <div className="col-span-full flex flex-wrap gap-4 text-sm">{(["embeddings_enabled","rag_enabled","overage_enabled","sso_enabled","sla_enabled","mtls_enabled","s3_api_enabled"] as const).map((field) => <label key={field} className="flex gap-2 items-center"><input type="checkbox" {...register(field)} />{field}</label>)}</div>
            <input type="hidden" {...register("price_multiplier")} /><input type="hidden" {...register("billing_mode")} /><input type="hidden" {...register("monthly_token_quota", { valueAsNumber: true })} /><input type="hidden" {...register("fair_use_policy")} /><input type="hidden" {...register("features_json")} />
          </CardContent>
        </Card>

        <div className="flex gap-3">
          <Button type="submit" disabled={updateMutation.isPending}>
            Guardar cambios
          </Button>
          {plan.is_active && (
            <Button variant="destructive" onClick={() => deactivateMutation.mutate()} disabled={deactivateMutation.isPending}>
              Desactivar plan
            </Button>
          )}
        </div>
      </form>
    </div>
  )
}
