"use client"

import { api } from "@/lib/api/client"

export interface AdminStorageFile {
  id: string
  user_id: string
  folder_id: string | null
  name: string
  mime_type: string
  size_bytes: number
  sha256: string
  status: string
  created_at: string
  updated_at: string
}

export interface AdminStorageFolder {
  id: string
  user_id: string
  parent_id: string | null
  name: string
  created_at: string
  updated_at: string
}

export interface AdminVectorUsage {
  user_id: string
  vector_count: number
  vector_bytes: number
  document_count: number
  collection_mode: string
  collection_name: string
  updated_at: string
  input_tokens: number
  cost_micros: number
  tokens_avoided: number
  completed_jobs: number
  failed_jobs: number
}

export interface AdminStorageUsage {
  user_id: string
  used_bytes: number
  quota_bytes: number
  remaining_bytes: number
  file_count: number
  max_files: number
  updated_at: string
}

export interface AdminIngestionSummary {
  pending: number
  processing: number
  ready: number
  failed: number
  dlq: number | null
  dlq_source: string
}

export interface AdminStorageSnapshot {
  user_id: string
  files: AdminStorageFile[]
  folders: AdminStorageFolder[]
  vector_usage: AdminVectorUsage
  storage_usage: AdminStorageUsage
  ingestion: AdminIngestionSummary
}

export const storageApi = {
  byUser: (userId: string) => api.get<AdminStorageSnapshot>(`/api/v1/admin/storage/users/${userId}`),
}
