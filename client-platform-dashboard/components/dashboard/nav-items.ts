import {
  LayoutDashboard,
  KeyRound,
  CreditCard,
  Wallet,
  Receipt,
  Boxes,
  HardDrive,
  FileSearch,
  UserCog,
  type LucideIcon,
} from "lucide-react"

export interface NavItem {
  href: string
  label: string
  icon: LucideIcon
}

export const NAV_ITEMS: NavItem[] = [
  { href: "/dashboard", label: "Panel", icon: LayoutDashboard },
  { href: "/api-keys", label: "API Keys", icon: KeyRound },
  { href: "/models", label: "Modelos", icon: Boxes },
  { href: "/storage", label: "Storage", icon: HardDrive },
  { href: "/rag", label: "RAG", icon: FileSearch },
  { href: "/subscription", label: "Suscripción", icon: CreditCard },
  { href: "/wallet", label: "Billetera", icon: Wallet },
  { href: "/payment-methods", label: "Métodos de pago", icon: CreditCard },
  { href: "/invoices", label: "Facturas", icon: Receipt },
  { href: "/profile", label: "Perfil", icon: UserCog },
]
