"use client"

import { useState } from "react"
import Link from "next/link"
import { useQuery } from "@tanstack/react-query"
import { Search } from "lucide-react"
import { modelsApi } from "@/lib/api/models"
import { Input } from "@/components/ui/input"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Badge } from "@/components/ui/badge"
import { Skeleton } from "@/components/ui/skeleton"

export default function ModelsPage() {
  const [search, setSearch] = useState("")
  const [provider, setProvider] = useState<string | undefined>()

  const { data, isLoading } = useQuery({
    queryKey: ["admin", "models", search, provider],
    queryFn: () => modelsApi.list({ search, active: undefined }),
  })

  const providers = data ? [...new Set(data.map((m) => m.provider))] : []

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">Modelos</h1>
        <p className="mt-1 text-sm text-muted-foreground">Gestiona el catálogo de modelos LLM.</p>
      </div>

      <div className="flex flex-col gap-4 sm:flex-row sm:items-center">
        <div className="relative flex-1">
          <Search className="absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
          <Input placeholder="Buscar modelo..." value={search} onChange={(e) => setSearch(e.target.value)} className="pl-9" />
        </div>
        <select
          className="rounded-lg border border-input bg-background px-3 py-2 text-sm"
          value={provider ?? ""}
          onChange={(e) => setProvider(e.target.value || undefined)}
        >
          <option value="">Todos los proveedores</option>
          {providers.map((p) => (
            <option key={p} value={p}>{p}</option>
          ))}
        </select>
      </div>

      <Card>
        <CardHeader>
          <CardTitle className="text-base">{data ? `${data.length} modelos` : "Modelos"}</CardTitle>
        </CardHeader>
        <CardContent className="p-0">
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-border text-left text-muted-foreground">
                  <th className="px-4 py-3 font-medium">Nombre</th>
                  <th className="px-4 py-3 font-medium">Proveedor</th>
                  <th className="px-4 py-3 font-medium">Familia</th>
                  <th className="px-4 py-3 font-medium">Input</th>
                  <th className="px-4 py-3 font-medium">Output</th>
                  <th className="px-4 py-3 font-medium">Estado</th>
                  <th className="px-4 py-3 font-medium" />
                </tr>
              </thead>
              <tbody>
                {isLoading ? (
                  Array.from({ length: 5 }).map((_, i) => (
                    <tr key={i} className="border-b border-border">
                      {Array.from({ length: 7 }).map((_, j) => (
                        <td key={j} className="px-4 py-3"><Skeleton className="h-4 w-20" /></td>
                      ))}
                    </tr>
                  ))
                ) : data?.length === 0 ? (
                  <tr>
                    <td colSpan={7} className="px-4 py-8 text-center text-sm text-muted-foreground">
                      No se encontraron modelos.
                    </td>
                  </tr>
                ) : (
                  data?.map((model) => (
                    <tr key={model.id} className="border-b border-border transition-colors hover:bg-accent/30">
                      <td className="px-4 py-3 font-medium">{model.display_name}</td>
                      <td className="px-4 py-3 text-muted-foreground">{model.provider}</td>
                      <td className="px-4 py-3 text-muted-foreground">{model.family}</td>
                      <td className="px-4 py-3">${model.pricing_input.toFixed(4)}</td>
                      <td className="px-4 py-3">${model.pricing_output.toFixed(4)}</td>
                      <td className="px-4 py-3">
                        <Badge variant={model.is_active ? "secondary" : "outline"}>
                          {model.is_active ? "Activo" : "Inactivo"}
                        </Badge>
                      </td>
                      <td className="px-4 py-3">
                        <Button variant="ghost" size="sm" render={<Link href={`/models/${model.id}`} />}>
                          Editar
                        </Button>
                      </td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </div>
        </CardContent>
      </Card>
    </div>
  )
}
