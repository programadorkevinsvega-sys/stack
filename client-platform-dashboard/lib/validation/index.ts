import { z } from "zod"

// Password strength: >=12 chars, 1 upper, 1 lower, 1 digit, 1 special.
export const passwordSchema = z
  .string()
  .min(12, "Debe tener al menos 12 caracteres")
  .regex(/[A-Z]/, "Debe incluir al menos una mayúscula")
  .regex(/[a-z]/, "Debe incluir al menos una minúscula")
  .regex(/[0-9]/, "Debe incluir al menos un dígito")
  .regex(/[^A-Za-z0-9]/, "Debe incluir al menos un carácter especial")

export const loginSchema = z.object({
  email: z.string().email("Correo inválido"),
  password: z.string().min(1, "La contraseña es obligatoria"),
  mfa_code: z
    .string()
    .optional()
    .refine((v) => !v || /^\d{6}$/.test(v), "El código debe tener 6 dígitos"),
})
export type LoginValues = z.infer<typeof loginSchema>

export const registerSchema = z
  .object({
    email: z.string().email("Correo inválido"),
    password: passwordSchema,
    confirm_password: z.string(),
    display_name: z.string().max(100, "Máximo 100 caracteres").optional().or(z.literal("")),
  })
  .refine((d) => d.password === d.confirm_password, {
    message: "Las contraseñas no coinciden",
    path: ["confirm_password"],
  })
export type RegisterValues = z.infer<typeof registerSchema>

export const createApiKeySchema = z.object({
  name: z.string().min(1, "El nombre es obligatorio").max(100, "Máximo 100 caracteres"),
})
export type CreateApiKeyValues = z.infer<typeof createApiKeySchema>

export const topUpSchema = z.object({
  amount: z
    .number({ message: "Ingresa un monto válido" })
    .positive("El monto debe ser mayor a 0")
    .refine((v) => Number.isFinite(v) && Math.round(v * 100) === v * 100, "Máximo 2 decimales"),
  currency: z.string().min(1, "Selecciona una moneda"),
})
export type TopUpValues = z.infer<typeof topUpSchema>

export const profileSchema = z.object({
  display_name: z.string().max(100, "Máximo 100 caracteres").optional().or(z.literal("")),
  avatar_url: z.string().url("URL inválida").optional().or(z.literal("")),
})
export type ProfileValues = z.infer<typeof profileSchema>

export const changePasswordSchema = z
  .object({
    old_password: z.string().min(1, "La contraseña actual es obligatoria"),
    new_password: passwordSchema,
    confirm_password: z.string(),
  })
  .refine((d) => d.new_password === d.confirm_password, {
    message: "Las contraseñas no coinciden",
    path: ["confirm_password"],
  })
export type ChangePasswordValues = z.infer<typeof changePasswordSchema>
