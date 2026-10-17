import {
  LayoutDashboard,
  Users,
  Boxes,
  CreditCard,
  Wallet,
  Receipt,
  Settings,
  HardDrive,
  type LucideIcon,
} from "lucide-react"

export interface NavItem {
  href: string
  label: string
  icon: LucideIcon
}

export const NAV_ITEMS: NavItem[] = [
  { href: "/dashboard", label: "Panel", icon: LayoutDashboard },
  { href: "/users", label: "Usuarios", icon: Users },
  { href: "/storage", label: "Almacenamiento", icon: HardDrive },
  { href: "/models", label: "Modelos", icon: Boxes },
  { href: "/plans", label: "Planes", icon: CreditCard },
  { href: "/wallets", label: "Billeteras", icon: Wallet },
  { href: "/invoices", label: "Facturas", icon: Receipt },
  { href: "/settings", label: "Configuración", icon: Settings },
]
