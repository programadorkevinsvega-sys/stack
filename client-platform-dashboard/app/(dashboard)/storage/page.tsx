"use client"

import { useRef, useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import Link from "next/link"
import { Download, FileText, Folder, FolderPlus, Pencil, Trash2, Upload, Search } from "lucide-react"
import { toast } from "sonner"
import { filesApi } from "@/lib/api/files"
import { subscriptionApi } from "@/lib/api/subscription"
import { ApiClientError } from "@/lib/api/client"
import type { FileRecord, FolderRecord } from "@/lib/types"
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

async function sha256(file: File) {
  const digest = await crypto.subtle.digest("SHA-256", await file.arrayBuffer())
  return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("")
}

function uploadWithProgress(url: string, headers: Record<string, string>, file: File, onProgress: (value: number) => void) {
  return new Promise<void>((resolve, reject) => {
    const request = new XMLHttpRequest()
    request.open("PUT", url)
    Object.entries(headers).forEach(([name, value]) => request.setRequestHeader(name, value))
    request.upload.onprogress = (event) => {
      if (event.lengthComputable) onProgress(Math.round((event.loaded / event.total) * 100))
    }
    request.onload = () => {
      if (request.status >= 200 && request.status < 300) resolve()
      else reject(new Error(`R2 rechazó la subida (${request.status})`))
    }
    request.onerror = () => reject(new Error("No se pudo completar la subida a R2"))
    request.send(file)
  })
}

export default function StoragePage() {
  const queryClient = useQueryClient()
  const inputReference = useRef<HTMLInputElement>(null)
  const [folderId, setFolderId] = useState<string | null>(null)
  const [folderName, setFolderName] = useState("")
  const [uploadProgress, setUploadProgress] = useState(0)
  const subscription = useQuery({ queryKey: ["subscription"], queryFn: subscriptionApi.get, retry: false })
  const hasStorageAccess = subscription.data?.status === "active" || subscription.data?.status === "trialing"
  const files = useQuery({ queryKey: ["files", folderId], queryFn: () => filesApi.list(folderId ?? undefined), enabled: hasStorageAccess })
  const folders = useQuery({ queryKey: ["folders", folderId], queryFn: () => filesApi.folders(folderId ?? undefined), enabled: hasStorageAccess })
  const storageUsage = useQuery({ queryKey: ["storage", "usage"], queryFn: filesApi.storageUsage, enabled: hasStorageAccess })

  const uploadMutation = useMutation({
    mutationFn: async (file: File) => {
      setUploadProgress(1)
      const record = await filesApi.create({ name: file.name, mime_type: file.type || "application/octet-stream", size_bytes: file.size, sha256: await sha256(file), folder_id: folderId ?? undefined })
      const upload = await filesApi.uploadUrl(record.id)
      await uploadWithProgress(upload.upload_url, upload.required_headers, file, setUploadProgress)
      return filesApi.confirm(record.id)
    },
    onSuccess: () => {
      setUploadProgress(100)
      toast.success("Archivo subido; la ingestión continuará en segundo plano")
      queryClient.invalidateQueries({ queryKey: ["files"] })
      queryClient.invalidateQueries({ queryKey: ["storage", "usage"] })
      queryClient.invalidateQueries({ queryKey: ["vectors", "usage"] })
      if (inputReference.current) inputReference.current.value = ""
    },
    onError: (error) => {
      setUploadProgress(0)
      toast.error(error instanceof ApiClientError ? error.message : error instanceof Error ? error.message : "No se pudo subir el archivo")
    },
  })
  const folderMutation = useMutation({
    mutationFn: () => filesApi.createFolder(folderName.trim(), folderId ?? undefined),
    onSuccess: () => { setFolderName(""); toast.success("Carpeta creada"); queryClient.invalidateQueries({ queryKey: ["folders"] }) },
    onError: (error) => toast.error(error instanceof Error ? error.message : "No se pudo crear la carpeta"),
  })
  const renameFolderMutation = useMutation({
    mutationFn: (input: { id: string; name: string }) => filesApi.renameFolder(input.id, input.name),
    onSuccess: () => { toast.success("Carpeta renombrada"); queryClient.invalidateQueries({ queryKey: ["folders"] }) },
    onError: (error) => toast.error(error instanceof Error ? error.message : "No se pudo renombrar la carpeta"),
  })
  const deleteFolderMutation = useMutation({
    mutationFn: (id: string) => filesApi.removeFolder(id),
    onSuccess: () => { toast.success("Carpeta eliminada"); queryClient.invalidateQueries({ queryKey: ["folders"] }) },
    onError: (error) => toast.error(error instanceof Error ? error.message : "No se pudo eliminar la carpeta"),
  })
  const renameMutation = useMutation({
    mutationFn: (input: { id: string; name: string }) => filesApi.rename(input.id, input.name),
    onSuccess: () => { toast.success("Archivo renombrado"); queryClient.invalidateQueries({ queryKey: ["files"] }) },
    onError: (error) => toast.error(error instanceof Error ? error.message : "No se pudo renombrar el archivo"),
  })
  const deleteMutation = useMutation({
    mutationFn: (id: string) => filesApi.remove(id),
    onSuccess: () => { toast.success("Archivo eliminado"); queryClient.invalidateQueries({ queryKey: ["files"] }); queryClient.invalidateQueries({ queryKey: ["storage", "usage"] }); queryClient.invalidateQueries({ queryKey: ["vectors", "usage"] }) },
    onError: (error) => toast.error(error instanceof Error ? error.message : "No se pudo eliminar el archivo"),
  })

  async function downloadFile(id: string) {
    try {
      const result = await filesApi.downloadUrl(id)
      window.open(result.download_url, "_blank", "noopener,noreferrer")
    } catch (error) {
      toast.error(error instanceof Error ? error.message : "No se pudo generar la descarga")
    }
  }

  if (subscription.isLoading) return <div className="space-y-6"><div><h1 className="text-2xl font-semibold tracking-tight">Storage</h1><p className="mt-1 text-sm text-muted-foreground">Comprobando el acceso de tu suscripción.</p></div><Skeleton className="h-32 w-full" /></div>
  if (!hasStorageAccess) return <div className="space-y-6"><div><h1 className="text-2xl font-semibold tracking-tight">Storage</h1><p className="mt-1 text-sm text-muted-foreground">Gestiona tus archivos privados y carpetas.</p></div><Card><CardHeader><CardTitle>Activa un plan para usar Storage</CardTitle><CardDescription>El almacenamiento requiere una suscripción activa o en prueba.</CardDescription></CardHeader><CardContent><Button render={<Link href="/plans" />}>Ver planes</Button></CardContent></Card></div>

  const currentFolder = folderId ? folders.data?.find((folder) => folder.id === folderId) : undefined
  return <div className="space-y-6">
    <div className="flex flex-wrap items-end justify-between gap-3"><div><h1 className="text-2xl font-semibold tracking-tight">Storage</h1><p className="mt-1 text-sm text-muted-foreground">Archivos, carpetas, cuotas y descargas seguras.</p></div><Button variant="outline" render={<Link href="/rag" />}><Search className="mr-2 size-4" />Abrir RAG</Button></div>
    <Card><CardHeader className="pb-2"><CardDescription>Almacenamiento usado</CardDescription><CardTitle>{storageUsage.isLoading ? <Skeleton className="h-7 w-24" /> : formatBytes(storageUsage.data?.used_bytes ?? 0)}</CardTitle></CardHeader><CardContent className="text-xs text-muted-foreground">{storageUsage.data?.quota_bytes ? `${formatBytes(storageUsage.data.remaining_bytes)} restantes de ${formatBytes(storageUsage.data.quota_bytes)}` : "Cuota sin límite"}</CardContent></Card>
    <div className="flex flex-col gap-3 rounded-lg border border-border bg-card p-4 sm:flex-row sm:items-center"><input ref={inputReference} type="file" className="hidden" onChange={(event) => { const file = event.target.files?.[0]; if (file) uploadMutation.mutate(file) }} /><Button onClick={() => inputReference.current?.click()} disabled={uploadMutation.isPending}><Upload className="mr-2 size-4" />{uploadMutation.isPending ? `Subiendo ${uploadProgress}%` : "Subir archivo"}</Button><div className="flex flex-1 gap-2"><Input value={folderName} onChange={(event) => setFolderName(event.target.value)} placeholder="Nueva carpeta" /><Button variant="outline" disabled={!folderName.trim() || folderMutation.isPending} onClick={() => folderMutation.mutate()}><FolderPlus className="mr-2 size-4" />Crear</Button></div></div>
    <Card><CardHeader><CardTitle className="text-base">Explorador de archivos</CardTitle><CardDescription>{currentFolder ? `Carpeta: ${currentFolder.name}` : "Raíz de tus archivos"}</CardDescription></CardHeader><CardContent className="space-y-4">
      {folderId && <Button variant="ghost" onClick={() => setFolderId(null)}>Volver a raíz</Button>}
      {folders.isLoading ? <Skeleton className="h-10 w-full" /> : folders.data?.length ? <div className="flex flex-wrap gap-2">{folders.data.map((folder: FolderRecord) => <div key={folder.id} className="flex items-center rounded-md border border-border"><Button variant="ghost" size="sm" onClick={() => setFolderId(folder.id)}><Folder className="mr-2 size-4" />{folder.name}</Button><Button variant="ghost" size="icon" title="Renombrar carpeta" onClick={() => { const name = window.prompt("Nuevo nombre", folder.name)?.trim(); if (name && name !== folder.name) renameFolderMutation.mutate({ id: folder.id, name }) }}><Pencil className="size-3" /></Button><Button variant="ghost" size="icon" title="Eliminar carpeta" onClick={() => { if (window.confirm(`¿Eliminar la carpeta ${folder.name}? Debe estar vacía.`)) deleteFolderMutation.mutate(folder.id) }}><Trash2 className="size-3 text-destructive" /></Button></div>)}</div> : null}
      <div className="overflow-x-auto rounded-lg border border-border"><Table><TableHeader><TableRow><TableHead>Archivo</TableHead><TableHead>Estado</TableHead><TableHead>Tamaño</TableHead><TableHead>Actualizado</TableHead><TableHead className="w-32">Acciones</TableHead></TableRow></TableHeader><TableBody>{files.isLoading ? [0, 1, 2].map((index) => <TableRow key={index}><TableCell><Skeleton className="h-4 w-40" /></TableCell><TableCell><Skeleton className="h-5 w-20" /></TableCell><TableCell><Skeleton className="h-4 w-16" /></TableCell><TableCell><Skeleton className="h-4 w-24" /></TableCell><TableCell /></TableRow>) : files.data?.length ? files.data.map((file: FileRecord) => <TableRow key={file.id}><TableCell><div className="flex items-center gap-2"><FileText className="size-4 text-muted-foreground" /><div><p className="font-medium">{file.name}</p><p className="text-xs text-muted-foreground">{file.mime_type}</p></div></div></TableCell><TableCell><Badge variant={file.status === "ready" ? "default" : file.status === "failed" ? "destructive" : "secondary"}>{file.status}</Badge></TableCell><TableCell>{formatBytes(file.size_bytes)}</TableCell><TableCell>{new Date(file.updated_at).toLocaleDateString()}</TableCell><TableCell><div className="flex gap-1"><Button variant="ghost" size="icon" title="Descargar" onClick={() => downloadFile(file.id)}><Download className="size-4" /></Button><Button variant="ghost" size="icon" title="Renombrar" onClick={() => { const name = window.prompt("Nuevo nombre", file.name)?.trim(); if (name && name !== file.name) renameMutation.mutate({ id: file.id, name }) }}><Pencil className="size-4" /></Button><Button variant="ghost" size="icon" title="Eliminar" onClick={() => { if (window.confirm(`¿Eliminar ${file.name}?`)) deleteMutation.mutate(file.id) }}><Trash2 className="size-4 text-destructive" /></Button></div></TableCell></TableRow>) : <TableRow><TableCell colSpan={5} className="py-12 text-center text-muted-foreground">No hay archivos en esta carpeta.</TableCell></TableRow>}</TableBody></Table></div>
    </CardContent></Card>
  </div>
}
