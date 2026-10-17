"use client"

import { useState } from "react"
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"
import { Plus, Copy, Trash2, KeyRound, RotateCw } from "lucide-react"
import { apiKeysApi } from "@/lib/api/apiKeys"
import { ApiClientError } from "@/lib/api/client"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { Badge } from "@/components/ui/badge"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog"
import { Skeleton } from "@/components/ui/skeleton"
import { createApiKeySchema } from "@/lib/validation"
import type { ApiKeyScope, CreatedApiKey } from "@/lib/types"

const AVAILABLE_SCOPES: Array<{ value: ApiKeyScope; label: string }> = [
  { value: "inference:read", label: "Leer modelos" },
  { value: "inference:write", label: "Usar modelos" },
  { value: "files:read", label: "Leer archivos" },
  { value: "files:write", label: "Subir archivos" },
  { value: "files:delete", label: "Eliminar archivos" },
]

const statusLabel: Record<string, string> = {
  active: "Activa",
  revoked: "Revocada",
  expired: "Expirada",
}

const statusVariant: Record<string, "default" | "secondary" | "outline"> = {
  active: "default",
  revoked: "secondary",
  expired: "outline",
}

export default function ApiKeysPage() {
  const qc = useQueryClient()
  const { data: keys, isLoading } = useQuery({
    queryKey: ["api-keys"],
    queryFn: apiKeysApi.list,
  })

  const [open, setOpen] = useState(false)
  const [name, setName] = useState("")
  const [error, setError] = useState("")
  const [created, setCreated] = useState<CreatedApiKey | null>(null)
  const [scopes, setScopes] = useState<ApiKeyScope[]>(["inference:write"])

  const createMut = useMutation({
    mutationFn: () => apiKeysApi.create(name, scopes),
    onSuccess: (data) => {
      setCreated(data)
      toast.success("API key creada")
      qc.invalidateQueries({ queryKey: ["api-keys"] })
    },
    onError: (e) => {
      setError(e instanceof ApiClientError ? e.message : "Error al crear key")
    },
  })

  const revokeMut = useMutation({
    mutationFn: (id: string) => apiKeysApi.revoke(id),
    onSuccess: () => {
      toast.success("API key revocada")
      qc.invalidateQueries({ queryKey: ["api-keys"] })
    },
    onError: (e) => {
      toast.error(e instanceof ApiClientError ? e.message : "Error al revocar key")
    },
  })

  const rotateMut = useMutation({
    mutationFn: (id: string) => apiKeysApi.rotate(id),
    onSuccess: (data) => {
      setCreated(data)
      setOpen(true)
      toast.success("API key rotada")
      qc.invalidateQueries({ queryKey: ["api-keys"] })
    },
    onError: (e) => {
      toast.error(e instanceof ApiClientError ? e.message : "Error al rotar key")
    },
  })

  function handleOpen() {
    setOpen(true)
    setName("")
    setError("")
    setCreated(null)
    setScopes(["inference:write"])
  }

  function handleCreate() {
    const parsed = createApiKeySchema.safeParse({ name })
    if (!parsed.success) {
      setError(parsed.error.issues[0].message)
      return
    }
    setError("")
    createMut.mutate()
  }

  function copyKey(val: string) {
    navigator.clipboard.writeText(val)
    toast.success("Key copiada al portapapeles")
  }

  function toggleScope(scope: ApiKeyScope) {
    setScopes((current) => current.includes(scope) ? current.filter((value) => value !== scope) : [...current, scope])
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">API Keys</h1>
          <p className="mt-1 text-sm text-muted-foreground">
            Gestiona las claves de acceso a la API.
          </p>
        </div>
          <Dialog open={open} onOpenChange={setOpen}>
           <DialogTrigger render={<Button onClick={handleOpen}><Plus className="mr-2 size-4" />Nueva key</Button>} />
          <DialogContent>
            <DialogHeader>
              <DialogTitle>{created ? "Key creada" : "Crear API key"}</DialogTitle>
              <DialogDescription>
                {created
                  ? "Copia esta key ahora. No podrás volver a verla."
                  : "Asigna un nombre descriptivo para identificar esta key."}
              </DialogDescription>
            </DialogHeader>
            {created ? (
              <div className="space-y-4">
                <div className="rounded-lg border border-border bg-muted/30 p-3">
                  <p className="mb-1 text-xs font-medium text-muted-foreground">Tu key</p>
                  <code className="break-all rounded bg-background px-2 py-1 text-sm font-mono">
                    {created.key_value}
                  </code>
                </div>
                <DialogFooter>
                  <Button onClick={() => copyKey(created.key_value)}>
                    <Copy className="mr-2 size-4" />
                    Copiar key
                  </Button>
                  <Button variant="outline" onClick={() => setOpen(false)}>
                    Cerrar
                  </Button>
                </DialogFooter>
              </div>
            ) : (
              <div className="space-y-4">
                <div className="flex flex-col gap-2">
                  <Label htmlFor="key-name">Nombre</Label>
                  <Input
                    id="key-name"
                    placeholder="p.ej. Producción, Staging, CI"
                    value={name}
                    onChange={(e) => setName(e.target.value)}
                    onKeyDown={(e) => e.key === "Enter" && handleCreate()}
                  />
                  {error && <p className="text-sm text-destructive">{error}</p>}
                </div>
                <fieldset className="space-y-2">
                  <legend className="text-sm font-medium">Permisos de la key</legend>
                  {AVAILABLE_SCOPES.map((scope) => <label key={scope.value} className="flex items-center gap-2 text-sm"><input type="checkbox" checked={scopes.includes(scope.value)} onChange={() => toggleScope(scope.value)} />{scope.label}<span className="ml-auto font-mono text-xs text-muted-foreground">{scope.value}</span></label>)}
                </fieldset>
                <DialogFooter>
                  <Button variant="outline" onClick={() => setOpen(false)}>
                    Cancelar
                  </Button>
                  <Button onClick={handleCreate} disabled={createMut.isPending}>
                    {createMut.isPending ? "Creando..." : "Crear key"}
                  </Button>
                </DialogFooter>
              </div>
            )}
          </DialogContent>
        </Dialog>
      </div>

      <div className="rounded-lg border border-border">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Nombre</TableHead>
              <TableHead>Prefijo</TableHead>
              <TableHead>Estado</TableHead>
              <TableHead>Scopes</TableHead>
              <TableHead>Creada</TableHead>
              <TableHead>Último uso</TableHead>
              <TableHead className="w-12" />
            </TableRow>
          </TableHeader>
          <TableBody>
            {isLoading ? (
              [0, 1, 2].map((index) => (
                <TableRow key={index}>
                  <TableCell><Skeleton className="h-4 w-24" /></TableCell>
                  <TableCell><Skeleton className="h-4 w-20" /></TableCell>
                  <TableCell><Skeleton className="h-5 w-16" /></TableCell>
                  <TableCell><Skeleton className="h-4 w-28" /></TableCell>
                  <TableCell><Skeleton className="h-4 w-28" /></TableCell>
                  <TableCell><Skeleton className="h-4 w-28" /></TableCell>
                  <TableCell />
                </TableRow>
              ))
            ) : keys?.length ? (
              keys.map((key) => (
                <TableRow key={key.id}>
                  <TableCell className="font-medium">{key.name}</TableCell>
                  <TableCell>
                    <code className="rounded bg-muted px-1.5 py-0.5 text-xs font-mono">
                      {key.key_prefix}...
                    </code>
                  </TableCell>
                  <TableCell>
                    <Badge variant={statusVariant[key.status] ?? "outline"}>
                      {statusLabel[key.status] ?? key.status}
                    </Badge>
                  </TableCell>
                  <TableCell className="max-w-56"><div className="flex flex-wrap gap-1">{key.scopes?.map((scope) => <Badge key={scope} variant="outline" className="text-[10px]">{scope}</Badge>) ?? <span className="text-xs text-muted-foreground">No disponibles</span>}</div></TableCell>
                  <TableCell className="text-sm text-muted-foreground">
                    {new Date(key.created_at).toLocaleDateString()}
                  </TableCell>
                  <TableCell className="text-sm text-muted-foreground">
                    {key.last_used_at ? new Date(key.last_used_at).toLocaleDateString() : "—"}
                  </TableCell>
                  <TableCell>
                    {key.status === "active" && <div className="flex items-center">
                      <Button
                        variant="ghost"
                        size="icon"
                        onClick={() => {
                          if (window.confirm("Rotar esta key revocará la actual. ¿Continuar?")) rotateMut.mutate(key.id)
                        }}
                        title="Rotar"
                        disabled={rotateMut.isPending}
                      >
                        <RotateCw className="size-4" />
                      </Button>
                      <Button
                        variant="ghost"
                        size="icon"
                        onClick={() => {
                          if (window.confirm("Revocar esta key?")) revokeMut.mutate(key.id)
                        }}
                        title="Revocar"
                        disabled={revokeMut.isPending}
                      >
                        <Trash2 className="size-4 text-destructive" />
                      </Button>
                    </div>}
                  </TableCell>
                </TableRow>
              ))
            ) : (
              <TableRow>
              <TableCell colSpan={7} className="py-12 text-center text-sm text-muted-foreground">
                  <KeyRound className="mx-auto mb-2 size-8 text-muted-foreground/50" />
                  No tienes API keys. Crea una para empezar.
                </TableCell>
              </TableRow>
            )}
          </TableBody>
        </Table>
      </div>
    </div>
  )
}
