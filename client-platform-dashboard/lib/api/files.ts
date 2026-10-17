"use client"

import { api, unwrapList } from "@/lib/api/client"
import type {
  DownloadUrlResponse,
  FileRecord,
  FolderRecord,
  UploadUrlResponse,
  VectorSearchHit,
  VectorUsage,
  StorageUsage,
} from "@/lib/types"

export const filesApi = {
  list: (folderId?: string) => {
    const suffix = folderId ? `?folder_id=${encodeURIComponent(folderId)}` : ""
    return api.get<FileRecord[] | { data: FileRecord[] }>(`/api/v1/files${suffix}`).then(unwrapList)
  },
  folders: (parentId?: string) => {
    const suffix = parentId ? `?parent_id=${encodeURIComponent(parentId)}` : ""
    return api.get<FolderRecord[] | { data: FolderRecord[] }>(`/api/v1/folders${suffix}`).then(unwrapList)
  },
  createFolder: (name: string, parentId?: string) =>
    api.post<FolderRecord>("/api/v1/folders", {
      name,
      parent_id: parentId ?? null,
      idempotency_key: crypto.randomUUID(),
    }),
  renameFolder: (id: string, name: string) => api.put<FolderRecord>(`/api/v1/folders/${id}`, { name }),
  removeFolder: (id: string) => api.delete<{ success: boolean }>(`/api/v1/folders/${id}`),
  create: (input: {
    name: string
    mime_type: string
    size_bytes: number
    sha256: string
    folder_id?: string
  }) =>
    api.post<FileRecord>("/api/v1/files", {
      ...input,
      idempotency_key: crypto.randomUUID(),
    }),
  uploadUrl: (id: string) =>
    api.post<UploadUrlResponse>(`/api/v1/files/${id}/upload-url`, { expires_seconds: 900 }),
  confirm: (id: string) => api.post<FileRecord>(`/api/v1/files/${id}/confirm`),
  downloadUrl: (id: string) =>
    api.post<DownloadUrlResponse>(`/api/v1/files/${id}/download-url`, { expires_seconds: 300 }),
  rename: (id: string, name: string) => api.put<FileRecord>(`/api/v1/files/${id}`, { name }),
  move: (id: string, folderId: string | null) =>
    api.put<FileRecord>(`/api/v1/files/${id}`, { folder_id: folderId }),
  remove: (id: string) => api.delete<{ success: boolean }>(`/api/v1/files/${id}`),
  reindex: (id: string) => api.post<{ accepted: boolean }>(`/api/v1/files/${id}/vectors/reindex`),
  vectorUsage: () => api.get<VectorUsage>("/api/v1/vectors/usage"),
  storageUsage: () => api.get<StorageUsage>("/api/v1/storage/usage"),
  search: (query: string, limit = 10) =>
    api.post<VectorSearchHit[]>("/api/v1/vectors/search", { query, limit }).then(unwrapList),
}
