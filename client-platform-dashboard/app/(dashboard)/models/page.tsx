"use client"

import { useState, useMemo } from "react"
import { useQuery } from "@tanstack/react-query"
import { Search, Boxes, Cpu, Braces, Eye, FileText, Workflow } from "lucide-react"
import { modelsApi } from "@/lib/api/models"
import { subscriptionApi } from "@/lib/api/subscription"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog"
import type { Model } from "@/lib/types"

const featureIcons: Record<string, typeof Cpu> = {
  chat: Braces,
  tools: Workflow,
  json_mode: FileText,
  vision: Eye,
  reasoning: Cpu,
}

const featureLabels: Record<string, string> = {
  chat: "Chat",
  tools: "Tools",
  json_mode: "JSON Mode",
  vision: "Visión",
  reasoning: "Razonamiento",
}

function ModelDetail({ model }: { model: Model }) {
  return (
    <div className="space-y-4">
      <div className="grid grid-cols-2 gap-3">
        <div>
          <p className="text-xs text-muted-foreground">ID</p>
          <p className="font-mono text-sm">{model.id}</p>
        </div>
        <div>
          <p className="text-xs text-muted-foreground">Proveedor</p>
          <p className="text-sm font-medium">{model.provider}</p>
        </div>
        <div>
          <p className="text-xs text-muted-foreground">Familia</p>
          <p className="text-sm">{model.family}</p>
        </div>
        <div>
          <p className="text-xs text-muted-foreground">Estado</p>
          <Badge variant={model.is_active ? "default" : "secondary"}>
            {model.is_active ? "Activo" : "Inactivo"}
          </Badge>
        </div>
      </div>
      <div className="grid grid-cols-2 gap-3 rounded-lg border border-border bg-muted/30 p-3">
        <div>
          <p className="text-xs text-muted-foreground">Input</p>
          <p className="font-medium">${model.pricing_input.toFixed(2)} / 1M tokens</p>
        </div>
        <div>
          <p className="text-xs text-muted-foreground">Output</p>
          <p className="font-medium">${model.pricing_output.toFixed(2)} / 1M tokens</p>
        </div>
      </div>
      {model.context_window && (
        <div>
          <p className="text-xs text-muted-foreground">Ventana de contexto</p>
          <p className="font-medium">{(model.context_window / 1000).toFixed(0)}K tokens</p>
        </div>
      )}
      {model.supported_features && model.supported_features.length > 0 && (
        <div>
          <p className="mb-2 text-xs text-muted-foreground">Features</p>
          <div className="flex flex-wrap gap-1.5">
            {model.supported_features.map((f) => (
              <Badge key={f} variant="outline" className="gap-1">
                <Cpu className="size-3" />
                {featureLabels[f] ?? f}
              </Badge>
            ))}
          </div>
        </div>
      )}
      {model.description && (
        <div>
          <p className="text-xs text-muted-foreground">Descripción</p>
          <p className="text-sm text-muted-foreground">{model.description}</p>
        </div>
      )}
    </div>
  )
}

export default function ModelsPage() {
  const subscription = useQuery({ queryKey: ["subscription"], queryFn: subscriptionApi.get, retry: false })
  const hasModelAccess = subscription.data?.status === "active" || subscription.data?.status === "trialing"
  const { data: models, isLoading } = useQuery({
    queryKey: ["models"],
    queryFn: modelsApi.list,
    enabled: hasModelAccess,
  })
  const [search, setSearch] = useState("")
  const [providerFilter, setProviderFilter] = useState<string>("")
  const [selected, setSelected] = useState<Model | null>(null)

  const providers = useMemo(() => {
    if (!models) return []
    return [...new Set(models.map((m) => m.provider))]
  }, [models])

  const filtered = useMemo(() => {
    if (!models) return []
    return models.filter((m) => {
      const matchesSearch =
        !search ||
        m.display_name.toLowerCase().includes(search.toLowerCase()) ||
        m.id.toLowerCase().includes(search.toLowerCase()) ||
        m.provider.toLowerCase().includes(search.toLowerCase())
      const matchesProvider = !providerFilter || m.provider === providerFilter
      return matchesSearch && matchesProvider
    })
  }, [models, search, providerFilter])

  if (subscription.isLoading) {
    return <div className="space-y-6"><h1 className="text-2xl font-semibold tracking-tight">Modelos</h1><Skeleton className="h-64 w-full" /></div>
  }

  if (!hasModelAccess) {
    return <div className="space-y-6"><h1 className="text-2xl font-semibold tracking-tight">Modelos</h1><Card><CardHeader><CardTitle>Activa un plan para ver los modelos</CardTitle></CardHeader><CardContent><Button render={<a href="/plans" />}>Ver planes</Button></CardContent></Card></div>
  }

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">Modelos</h1>
        <p className="mt-1 text-sm text-muted-foreground">
          Catálogo de modelos disponibles con precios por token.
        </p>
      </div>

      <div className="flex flex-col gap-3 sm:flex-row">
        <div className="relative flex-1">
          <Search className="absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
          <Input
            placeholder="Buscar modelos..."
            className="pl-9"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
        </div>
        <div className="flex flex-wrap gap-2">
          <Button
            variant={providerFilter === "" ? "default" : "outline"}
            size="sm"
            onClick={() => setProviderFilter("")}
          >
            Todos
          </Button>
          {providers.map((p) => (
            <Button
              key={p}
              variant={providerFilter === p ? "default" : "outline"}
              size="sm"
              onClick={() => setProviderFilter(providerFilter === p ? "" : p)}
            >
              {p}
            </Button>
          ))}
        </div>
      </div>

      {isLoading ? (
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {Array.from({ length: 3 }).map((_, i) => (
            <Card key={i}>
              <CardHeader><Skeleton className="h-5 w-24" /></CardHeader>
              <CardContent className="space-y-2">
                <Skeleton className="h-4 w-32" />
                <Skeleton className="h-4 w-20" />
                <Skeleton className="h-4 w-28" />
              </CardContent>
            </Card>
          ))}
        </div>
      ) : filtered.length ? (
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {filtered.map((model) => (
            <Card key={model.id} className="relative">
              <CardHeader>
                <div className="flex items-start justify-between">
                  <div>
                    <CardTitle className="text-base">{model.display_name}</CardTitle>
                    <p className="mt-0.5 text-xs text-muted-foreground">{model.provider} / {model.family}</p>
                  </div>
                  <Badge variant={model.is_active ? "default" : "secondary"}>
                    {model.is_active ? "Activo" : "Inactivo"}
                  </Badge>
                </div>
              </CardHeader>
              <CardContent className="space-y-3">
                <div className="flex gap-3 text-sm">
                  <div>
                    <p className="text-xs text-muted-foreground">Input</p>
                    <p className="font-medium">${model.pricing_input.toFixed(2)}</p>
                  </div>
                  <div>
                    <p className="text-xs text-muted-foreground">Output</p>
                    <p className="font-medium">${model.pricing_output.toFixed(2)}</p>
                  </div>
                  {model.context_window && (
                    <div>
                      <p className="text-xs text-muted-foreground">Contexto</p>
                      <p className="font-medium">{(model.context_window / 1000).toFixed(0)}K</p>
                    </div>
                  )}
                </div>
                {model.supported_features && (
                  <div className="flex flex-wrap gap-1">
                    {model.supported_features.slice(0, 3).map((f) => (
                      <Badge key={f} variant="outline" className="text-[10px]">
                        {featureLabels[f] ?? f}
                      </Badge>
                    ))}
                    {model.supported_features.length > 3 && (
                      <Badge variant="outline" className="text-[10px]">
                        +{model.supported_features.length - 3}
                      </Badge>
                    )}
                  </div>
                )}
                {model.description && (
                  <p className="text-xs text-muted-foreground line-clamp-2">{model.description}</p>
                )}
                <Dialog>
                  <DialogTrigger render={<Button
                    variant="outline"
                    size="sm"
                    className="w-full"
                    onClick={() => setSelected(model)}
                  >Ver detalle</Button>} />
                  <DialogContent>
                    <DialogHeader>
                      <DialogTitle>{model.display_name}</DialogTitle>
                    </DialogHeader>
                    <ModelDetail model={selected ?? model} />
                  </DialogContent>
                </Dialog>
              </CardContent>
            </Card>
          ))}
        </div>
      ) : (
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Boxes className="size-5 text-muted-foreground" />
              Sin resultados
            </CardTitle>
          </CardHeader>
          <CardContent>
            <p className="text-sm text-muted-foreground">
              {search || providerFilter
                ? "Ningún modelo coincide con los filtros aplicados."
                : "No hay modelos disponibles en este momento."}
            </p>
          </CardContent>
        </Card>
      )}
    </div>
  )
}
