import { z } from "zod"

export const loginSchema = z.object({
  email: z.string().email("Correo inválido"),
  password: z.string().min(1, "La contraseña es obligatoria"),
  mfa_code: z.string().optional().refine((v) => !v || /^\d{6}$/.test(v), "El código debe tener 6 dígitos"),
})
export type LoginValues = z.infer<typeof loginSchema>

export const createPlanSchema = z.object({
  name: z.string().min(1, "El nombre es obligatorio").max(100),
  description: z.string().max(2000).optional(),
  price: z.number().finite().nonnegative().optional(),
  interval: z.enum(["month", "year"]).optional(),
  slug: z.string().min(1).max(100).regex(/^[a-z0-9-]+$/).optional(),
  requests_per_minute: z.number().int().positive().optional(),
  tokens_per_minute: z.number().int().positive().optional(),
  max_concurrent_requests: z.number().int().positive().optional(),
  monthly_token_quota: z.number().int().nonnegative().nullable().optional(),
  price_multiplier: z.string().regex(/^\d+(\.\d{1,6})?$/).optional(),
  billing_mode: z.enum(["prepaid", "postpaid"]).optional(),
  storage_bytes_quota: z.number().int().nonnegative().optional(),
  max_file_size_bytes: z.number().int().nonnegative().optional(),
  max_files: z.number().int().nonnegative().optional(),
  embeddings_enabled: z.boolean().optional(), rag_enabled: z.boolean().optional(),
  qdrant_mode: z.enum(["none", "shared", "dedicated"]).optional(),
  price_amount: z.string().regex(/^\d+(\.\d{1,2})?$/).optional(),
  currency: z.string().length(3).optional(), trial_days: z.number().int().nonnegative().optional(),
  max_api_keys: z.number().int().nonnegative().optional(), overage_enabled: z.boolean().optional(),
  fair_use_policy: z.string().max(2000).optional(), grace_period_days: z.number().int().nonnegative().optional(),
  sso_enabled: z.boolean().optional(), sla_enabled: z.boolean().optional(), mtls_enabled: z.boolean().optional(),
  s3_api_enabled: z.boolean().optional(), features_json: z.string().optional(),
})
export type CreatePlanValues = z.infer<typeof createPlanSchema>

export const adjustWalletSchema = z.object({
  amount: z.number().finite("Monto inválido"),
  reason: z.string().min(1, "La razón es obligatoria").max(500),
})
export type AdjustWalletValues = z.infer<typeof adjustWalletSchema>
