export interface ApiError {
  error: true
  message: string
  fields?: Record<string, string>
}

export interface AdminUser {
  id: string
  email: string
  display_name: string | null
  email_verified: boolean
  mfa_enabled: boolean
  status: "active" | "disabled" | "locked"
  role: "user" | "admin"
  created_at: string
  last_login_at: string | null
}

export interface AdminLoginResponse {
  user: AdminUser
  refresh_token?: string
}

export interface AdminModel {
  id: string
  display_name: string
  provider: string
  provider_model_id?: string
  family: string
  pricing_input: number
  pricing_output: number
  is_active: boolean
  context_window?: number
  max_output_tokens?: number
  supported_features?: string[]
  description?: string
}

export interface AdminPlan {
  id: string
  name: string
  description: string
  price: number
  currency: string
  interval: "month" | "year"
  features: string[]
  is_active: boolean
  highlighted?: boolean
  slug?: string
  requests_per_minute?: number
  tokens_per_minute?: number
  max_concurrent_requests?: number
  monthly_token_quota?: number | null
  price_multiplier?: string
  billing_mode?: "prepaid" | "postpaid"
  storage_bytes_quota?: number
  max_file_size_bytes?: number
  max_files?: number
  embeddings_enabled?: boolean
  rag_enabled?: boolean
  qdrant_mode?: "none" | "shared" | "dedicated"
  price_amount?: string
  trial_days?: number
  max_api_keys?: number
  overage_enabled?: boolean
  fair_use_policy?: string
  grace_period_days?: number
  sso_enabled?: boolean
  sla_enabled?: boolean
  mtls_enabled?: boolean
  s3_api_enabled?: boolean
}

export type InvoiceStatus = "paid" | "open" | "void" | "uncollectible"

export interface AdminInvoice {
  id: string
  user_id: string
  user_email: string
  amount: number
  currency: string
  status: InvoiceStatus
  due_date: string
  paid_at: string | null
  invoice_url: string | null
}

export interface AdminInvoiceDetail extends AdminInvoice {
  line_items: Array<{
    description: string
    quantity: number
    unit_price: number
    amount: number
  }>
}

export interface AdminWallet {
  user_id: string
  user_email: string
  balance: number
  currency: string
  credit_limit: number
}

export interface UsageSummary {
  total_users: number
  total_requests_30d: number
  total_tokens_30d: number
  total_revenue_30d: number
  active_subscriptions: number
  error_rate_30d: number
}
