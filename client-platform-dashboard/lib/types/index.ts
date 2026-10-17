// Canonical TypeScript types for all Customer BFF entities.
// These mirror the JSON contracts documented in the project spec.

export type PaymentProvider = "stripe" | "paypal" | "payoneer" | "wompi" | "payu"

export interface ApiError {
  error: true
  message: string
  // Optional field-level validation errors for 422 responses.
  fields?: Record<string, string>
}

// ----- Auth / User -----

export interface User {
  id: string
  email: string
  display_name: string | null
  avatar_url: string | null
  mfa_enabled: boolean
  email_verified: boolean
  created_at: string
}

export interface ActiveSession {
  session_id: string
  user_agent: string
  ip_address: string
  created_at: string
  last_seen_at: string
}

export interface LoginResponse {
  user: User
  // refresh_token also arrives via the x-refresh-token response header.
  refresh_token?: string
  mfa_required?: boolean
}

export interface EnableMfaResponse {
  otpauth_uri: string
  secret: string
}

// ----- Usage -----

export interface UsageSummary {
  requests: number
  tokens_in: number
  tokens_out: number
  tokens_cached: number
  error_count: number
  error_rate: number
  avg_latency_ms: number
  // Daily series for charting.
  series: Array<{ date: string; requests: number; tokens: number }>
}

export interface TopModel {
  model_id: string
  display_name: string
  requests: number
  tokens: number
  cost: number
}

export interface LatencyStats {
  p50: number
  p95: number
  p99: number
  min: number
  avg: number
  max: number
}

// ----- API Keys -----

export interface ApiKey {
  id: string
  name: string
  key_prefix: string
  created_at: string
  expires_at: string | null
  status: "active" | "revoked" | "expired"
  last_used_at: string | null
  scopes?: ApiKeyScope[]
}

export interface CreatedApiKey extends ApiKey {
  key_value: string
}

export type ApiKeyScope =
  | "inference:read"
  | "inference:write"
  | "keys:manage"
  | "billing:read"
  | "files:read"
  | "files:write"
  | "files:delete"

export interface FileRecord {
  id: string
  user_id: string
  folder_id: string | null
  name: string
  mime_type: string
  size_bytes: number
  sha256: string
  status: string
  object_key: string
  created_at: string
  updated_at: string
}

export interface FolderRecord {
  id: string
  user_id: string
  parent_id: string | null
  name: string
  created_at: string
  updated_at: string
}

export interface UploadUrlResponse {
  file: FileRecord
  upload_url: string
  required_headers: Record<string, string>
  expires_at: string
}

export interface DownloadUrlResponse {
  file: FileRecord
  download_url: string
  expires_at: string
}

export interface VectorUsage {
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

export interface StorageUsage {
  user_id: string
  used_bytes: number
  quota_bytes: number
  remaining_bytes: number
  file_count: number
  max_files: number
  updated_at: string
}

export interface VectorSearchHit {
  chunk_id: string
  document_id: string
  file_id: string
  chunk_index: number
  content: string
  content_hash: string
  embedding_model: string
  embedding_version: string
  score: number
}

// ----- Plans / Subscription -----

export interface Plan {
  id: string
  name: string
  description: string
  price: number
  currency: string
  trial_days?: number
  interval: "month" | "year"
  features: string[]
  is_active: boolean
  highlighted?: boolean
}

export type SubscriptionStatus =
  | "active"
  | "trialing"
  | "past_due"
  | "canceled"
  | "incomplete"
  | "processing"

export interface Subscription {
  plan_id: string
  plan_name: string
  status: SubscriptionStatus
  current_period_start: string
  current_period_end: string
  cancel_at_period_end: boolean
  billing_cycle: "month" | "year"
}

export interface RedirectResponse {
  redirect_url: string
  flow?: "paypal_vault_setup" | string
  plan_id?: string
  trial_days?: number
}

// ----- Wallet -----

export interface Wallet {
  balance: number
  currency: string
  credit_limit: number
}

// ----- Payment methods -----

export interface PaymentMethod {
  id: string
  type: PaymentProvider | "card"
  last_four: string | null
  expiry_month: number | null
  expiry_year: number | null
  is_default: boolean
}

// ----- Invoices -----

export interface InvoiceLineItem {
  description: string
  quantity: number
  unit_price: number
  amount: number
}

export interface Invoice {
  id: string
  amount: number
  currency: string
  status: "paid" | "open" | "void" | "uncollectible"
  due_date: string
  paid_at: string | null
  invoice_url: string | null
  line_items?: InvoiceLineItem[]
}

// ----- Models catalog -----

export interface Model {
  id: string
  display_name: string
  provider: string
  family: string
  pricing_input: number
  pricing_output: number
  is_active: boolean
  context_window?: number
  max_output_tokens?: number
  supported_features?: string[]
  description?: string
}
