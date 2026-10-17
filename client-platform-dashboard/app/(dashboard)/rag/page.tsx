"use client"

import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import Link from "next/link"
import { Database, FileSearch, HardDrive, RefreshCw, Search } from "lucide-react"
import { toast } from "sonner"
import { filesApi } from "@/lib/api/files"
import { subscriptionApi } from "@/lib/api/subscription"
import type { FileRecord, VectorSearchHit } from "@/lib/types"
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

export default function RagPage() {
  const queryClient = useQueryClient()
  const [searchText, setSearchText] = useState("")
  const [searchResults, setSearchResults] = useState<VectorSearchHit[]>([])
  const subscription = useQuery({ queryKey: ["subscription"], queryFn: subscriptionApi.get, retry: false })
  const hasRagAccess = subscription.data?.status === "active" || subscription.data?.status === "trialing"
  const vectorUsage = useQuery({ queryKey: ["vectors", "usage"], queryFn: filesApi.vectorUsage, enabled: hasRagAccess })
  const files = useQuery({ queryKey: ["files", "rag-status"], queryFn: () => filesApi.list(), enabled: hasRagAccess })
  const searchMutation = useMutation({ mutationFn: () => filesApi.search(searchText.trim()), onSuccess: setSearchResults, onError: (error) => toast.error(error instanceof Error ? error.message : "No se pudo buscar en RAG") })
  const reindexMutation = useMutation({ mutationFn: (id: string) => filesApi.reindex(id), onSuccess: () => { toast.success("Reindexación aceptada"); queryClient.invalidateQueries({ queryKey: ["vectors", "usage"] }); queryClient.invalidateQueries({ queryKey: ["files", "rag-status"] }) }, onError: (error) => toast.error(error instanceof Error ? error.message : "No se pudo reindexar") })

  if (subscription.isLoading) return <div className="space-y-6"><div><h1 className="text-2xl font-semibold tracking-tight">RAG</h1><p className="mt-1 text-sm text-muted-foreground">Comprobando el acceso de tu suscripción.</p></div><Skeleton className="h-32 w-full" /></div>
  if (!hasRagAccess) return <div className="space-y-6"><div><h1 className="text-2xl font-semibold tracking-tight">RAG</h1><p className="mt-1 text-sm text-muted-foreground">Búsqueda semántica, embeddings e ingestión.</p></div><Card><CardHeader><CardTitle>Activa un plan para usar RAG</CardTitle><CardDescription>La ingestión y búsqueda vectorial requieren una suscripción activa o en prueba.</CardDescription></CardHeader><CardContent><Button render={<Link href="/plans" />}>Ver planes</Button></CardContent></Card></div>

  const usage = vectorUsage.data
  return <div className="space-y-6">
    <div className="flex flex-wrap items-end justify-between gap-3"><div><h1 className="text-2xl font-semibold tracking-tight">RAG</h1><p className="mt-1 text-sm text-muted-foreground">Embeddings, ingestión, estado de indexación y búsqueda semántica.</p></div><Button variant="outline" render={<Link href="/storage" />}><HardDrive className="mr-2 size-4" />Abrir Storage</Button></div>
    <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-5"><Card><CardHeader className="pb-2"><CardDescription>Vectores canónicos</CardDescription><CardTitle>{vectorUsage.isLoading ? <Skeleton className="h-7 w-20" /> : (usage?.vector_count ?? 0).toLocaleString()}</CardTitle></CardHeader><CardContent className="text-xs text-muted-foreground">{usage?.collection_mode ?? "Cargando colección"}</CardContent></Card><Card><CardHeader className="pb-2"><CardDescription>Documentos indexados</CardDescription><CardTitle>{(usage?.document_count ?? 0).toLocaleString()}</CardTitle></CardHeader><CardContent className="text-xs text-muted-foreground">Modelo Qwen3</CardContent></Card><Card><CardHeader className="pb-2"><CardDescription>Bytes vectoriales</CardDescription><CardTitle>{formatBytes(usage?.vector_bytes ?? 0)}</CardTitle></CardHeader><CardContent className="text-xs text-muted-foreground">{usage?.collection_name ?? ""}</CardContent></Card><Card><CardHeader className="pb-2"><CardDescription>Tokens procesados</CardDescription><CardTitle>{(usage?.input_tokens ?? 0).toLocaleString()}</CardTitle></CardHeader><CardContent className="text-xs text-muted-foreground">{usage?.completed_jobs ?? 0} trabajos completados</CardContent></Card><Card><CardHeader className="pb-2"><CardDescription>Tokens evitados</CardDescription><CardTitle>{(usage?.tokens_avoided ?? 0).toLocaleString()}</CardTitle></CardHeader><CardContent className="text-xs text-muted-foreground">Deduplicación por hash</CardContent></Card></div>
    <Card><CardHeader><CardTitle className="text-base">Buscar en tus documentos</CardTitle><CardDescription>Cada consulta aplica el aislamiento obligatorio de tu identidad en el backend.</CardDescription></CardHeader><CardContent className="space-y-3"><div className="flex gap-2"><Input value={searchText} onChange={(event) => setSearchText(event.target.value)} placeholder="Pregunta sobre tus documentos" onKeyDown={(event) => { if (event.key === "Enter" && searchText.trim()) searchMutation.mutate() }} /><Button onClick={() => searchMutation.mutate()} disabled={!searchText.trim() || searchMutation.isPending}><Search className="mr-2 size-4" />Buscar</Button></div>{searchResults.length ? searchResults.map((result) => <div key={`${result.file_id}:${result.chunk_id}`} className="rounded-lg border border-border p-3"><div className="flex items-center justify-between gap-3"><p className="text-sm font-medium">{files.data?.find((file) => file.id === result.file_id)?.name ?? result.file_id}</p><Badge variant="outline">{result.score.toFixed(3)}</Badge></div><p className="mt-2 text-sm text-muted-foreground">{result.content}</p><p className="mt-2 text-xs text-muted-foreground">{result.embedding_model} · {result.embedding_version}</p></div>) : <p className="text-sm text-muted-foreground">Aún no hay resultados.</p>}</CardContent></Card>
    <Card><CardHeader><CardTitle className="text-base">Estado de ingestión</CardTitle><CardDescription>Reprocesa únicamente los archivos que necesiten actualizar sus embeddings.</CardDescription></CardHeader><CardContent><div className="overflow-x-auto rounded-lg border border-border"><Table><TableHeader><TableRow><TableHead>Archivo</TableHead><TableHead>Estado</TableHead><TableHead>Actualizado</TableHead><TableHead className="w-24">Acción</TableHead></TableRow></TableHeader><TableBody>{files.isLoading ? [0, 1, 2].map((index) => <TableRow key={index}><TableCell><Skeleton className="h-4 w-40" /></TableCell><TableCell><Skeleton className="h-5 w-20" /></TableCell><TableCell><Skeleton className="h-4 w-24" /></TableCell><TableCell /></TableRow>) : files.data?.length ? files.data.map((file: FileRecord) => <TableRow key={file.id}><TableCell className="font-medium"><div className="flex items-center gap-2"><FileSearch className="size-4 text-muted-foreground" />{file.name}</div></TableCell><TableCell><Badge variant={file.status === "ready" ? "default" : file.status === "failed" ? "destructive" : "secondary"}>{file.status}</Badge></TableCell><TableCell>{new Date(file.updated_at).toLocaleString()}</TableCell><TableCell><Button variant="ghost" size="icon" title="Reindexar" onClick={() => reindexMutation.mutate(file.id)} disabled={reindexMutation.isPending}><RefreshCw className="size-4" /></Button></TableCell></TableRow>) : <TableRow><TableCell colSpan={4} className="py-12 text-center text-muted-foreground">Sube documentos desde Storage para iniciar la ingestión.</TableCell></TableRow>}</TableBody></Table></div></CardContent></Card>
  </div>
}
