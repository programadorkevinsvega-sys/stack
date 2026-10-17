"use client"

import { useState } from "react"
import { useQuery } from "@tanstack/react-query"
import { HardDrive, Search } from "lucide-react"
import { usersApi } from "@/lib/api/users"
import { storageApi } from "@/lib/api/storage"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { Skeleton } from "@/components/ui/skeleton"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`
}

export default function AdminStoragePage() {
  const [search, setSearch] = useState("")
  const [selectedUser, setSelectedUser] = useState<string | null>(null)
  const users = useQuery({ queryKey: ["admin", "users", "storage"], queryFn: () => usersApi.list({ search: search || undefined }) })
  const snapshot = useQuery({
    queryKey: ["admin", "storage", selectedUser],
    queryFn: () => storageApi.byUser(selectedUser as string),
    enabled: selectedUser !== null,
  })

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">Almacenamiento e ingestión</h1>
        <p className="mt-1 text-sm text-muted-foreground">Consulta capacidad, documentos, vectores y estados de ingestión por usuario.</p>
      </div>
      <Card>
        <CardHeader><CardTitle className="text-base">Seleccionar usuario</CardTitle><CardDescription>Las consultas se resuelven en el Admin BFF y se validan por user_id.</CardDescription></CardHeader>
        <CardContent className="space-y-3">
          <div className="flex gap-2"><Input value={search} onChange={(event) => setSearch(event.target.value)} placeholder="Buscar por correo" /><Button variant="outline" onClick={() => users.refetch()}><Search className="mr-2 size-4" />Buscar</Button></div>
          {users.isLoading ? <Skeleton className="h-10 w-full" /> : <div className="grid gap-2 md:grid-cols-2 lg:grid-cols-3">{users.data?.map((user) => <Button key={user.id} variant={selectedUser === user.id ? "default" : "outline"} className="justify-start" onClick={() => setSelectedUser(user.id)}><HardDrive className="mr-2 size-4" /><span className="truncate">{user.email}</span></Button>)}</div>}
        </CardContent>
      </Card>
      {selectedUser && snapshot.isLoading && <Skeleton className="h-72 w-full" />}
      {snapshot.data && <div className="space-y-6">
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-6">
          <Card><CardHeader className="pb-2"><CardDescription>Almacenamiento</CardDescription><CardTitle>{formatBytes(snapshot.data.storage_usage.used_bytes)}</CardTitle></CardHeader><CardContent className="text-xs text-muted-foreground">{snapshot.data.storage_usage.quota_bytes ? `${formatBytes(snapshot.data.storage_usage.remaining_bytes)} restantes` : "Cuota sin límite"}</CardContent></Card>
          <Card><CardHeader className="pb-2"><CardDescription>Archivos</CardDescription><CardTitle>{snapshot.data.files.length}</CardTitle></CardHeader></Card>
          <Card><CardHeader className="pb-2"><CardDescription>Carpetas</CardDescription><CardTitle>{snapshot.data.folders.length}</CardTitle></CardHeader></Card>
          <Card><CardHeader className="pb-2"><CardDescription>Vectores canónicos</CardDescription><CardTitle>{snapshot.data.vector_usage.vector_count.toLocaleString()}</CardTitle></CardHeader></Card>
          <Card><CardHeader className="pb-2"><CardDescription>Bytes vectoriales</CardDescription><CardTitle>{formatBytes(snapshot.data.vector_usage.vector_bytes)}</CardTitle></CardHeader></Card>
          <Card><CardHeader className="pb-2"><CardDescription>Colección</CardDescription><CardTitle className="truncate text-base">{snapshot.data.vector_usage.collection_name}</CardTitle></CardHeader><CardContent><Badge variant="outline">{snapshot.data.vector_usage.collection_mode}</Badge></CardContent></Card>
          <Card><CardHeader className="pb-2"><CardDescription>Embeddings</CardDescription><CardTitle>{snapshot.data.vector_usage.input_tokens.toLocaleString()} tokens</CardTitle></CardHeader><CardContent className="text-xs text-muted-foreground">{snapshot.data.vector_usage.completed_jobs} completados · {snapshot.data.vector_usage.failed_jobs} fallidos</CardContent></Card>
        </div>
        <Card><CardHeader><CardTitle className="text-base">Estado de ingestión</CardTitle><CardDescription>Fallos son estados persistidos en file-service; DLQ proviene del estado de EVENTS_DLQ en JetStream.</CardDescription></CardHeader><CardContent className="grid gap-4 sm:grid-cols-2 lg:grid-cols-5 text-sm"><div><span className="text-muted-foreground">Pendientes</span><p className="text-xl font-semibold">{snapshot.data.ingestion.pending}</p></div><div><span className="text-muted-foreground">Procesando</span><p className="text-xl font-semibold">{snapshot.data.ingestion.processing}</p></div><div><span className="text-muted-foreground">Listos</span><p className="text-xl font-semibold">{snapshot.data.ingestion.ready}</p></div><div><span className="text-muted-foreground">Fallidos</span><p className="text-xl font-semibold">{snapshot.data.ingestion.failed}</p></div><div><span className="text-muted-foreground">DLQ</span><p className="text-xl font-semibold">{snapshot.data.ingestion.dlq ?? 0}</p></div></CardContent></Card>
        <Card><CardHeader><CardTitle className="text-base">Documentos</CardTitle></CardHeader><CardContent><div className="overflow-x-auto rounded-lg border border-border"><Table><TableHeader><TableRow><TableHead>Nombre</TableHead><TableHead>MIME</TableHead><TableHead>Tamaño</TableHead><TableHead>Estado</TableHead><TableHead>Actualizado</TableHead></TableRow></TableHeader><TableBody>{snapshot.data.files.length ? snapshot.data.files.map((file) => <TableRow key={file.id}><TableCell className="font-medium">{file.name}</TableCell><TableCell>{file.mime_type}</TableCell><TableCell>{formatBytes(file.size_bytes)}</TableCell><TableCell><Badge variant={file.status === "failed" ? "destructive" : file.status === "ready" ? "default" : "secondary"}>{file.status}</Badge></TableCell><TableCell>{new Date(file.updated_at).toLocaleString()}</TableCell></TableRow>) : <TableRow><TableCell colSpan={5} className="py-10 text-center text-muted-foreground">No hay documentos para este usuario.</TableCell></TableRow>}</TableBody></Table></div></CardContent></Card>
      </div>}
    </div>
  )
}
