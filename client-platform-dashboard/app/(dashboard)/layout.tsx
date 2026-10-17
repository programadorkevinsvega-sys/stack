"use client"

import { useEffect, useState } from "react"
import Link from "next/link"
import { usePathname, useRouter } from "next/navigation"
import { Menu, X, LogOut, ChevronDown } from "lucide-react"
import { useQuery } from "@tanstack/react-query"
import { authApi } from "@/lib/api/auth"
import { subscriptionApi } from "@/lib/api/subscription"
import { ApiClientError } from "@/lib/api/client"
import { useSession } from "@/lib/store/session"
import { BrandMark } from "@/components/brand"
import { ThemeToggle } from "@/components/theme-toggle"
import { NAV_ITEMS } from "@/components/dashboard/nav-items"
import { Button } from "@/components/ui/button"
import { Avatar, AvatarFallback } from "@/components/ui/avatar"
import { Skeleton } from "@/components/ui/skeleton"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { ErrorBoundary } from "@/components/error-boundary"
import { cn } from "@/lib/utils"

function UserAvatar({ name, email }: { name: string | null; email: string }) {
  const initial = (name ?? email).charAt(0).toUpperCase()
  return (
    <Avatar className="size-8">
      <AvatarFallback className="text-xs">{initial}</AvatarFallback>
    </Avatar>
  )
}

export default function DashboardLayout({ children }: { children: React.ReactNode }) {
  const pathname = usePathname()
  const router = useRouter()
  const { user, setSession, initialized, clear } = useSession()
  const [sidebarOpen, setSidebarOpen] = useState(false)

  const meQuery = useQuery({
    queryKey: ["me"],
    queryFn: authApi.me,
    enabled: !initialized,
    retry: false,
    meta: { silent401: true },
  })

  const subscriptionQuery = useQuery({
    queryKey: ["subscription"],
    queryFn: subscriptionApi.get,
    enabled: initialized && user !== null,
    retry: false,
    staleTime: 60_000,
    meta: { silent401: true },
  })

  useEffect(() => {
    if (meQuery.data) setSession(meQuery.data, null)
  }, [meQuery.data, setSession])

  useEffect(() => {
    const meUnauthorized = meQuery.error instanceof ApiClientError && meQuery.error.status === 401
    if (meUnauthorized) {
      clear()
      router.replace(`/login?redirect=${encodeURIComponent(pathname)}`)
      return
    }
    const noSubscription = subscriptionQuery.error instanceof ApiClientError && subscriptionQuery.error.status === 404
    if (!initialized || meQuery.isLoading || !user || subscriptionQuery.isLoading || (subscriptionQuery.isError && !noSubscription)) return
    const active = subscriptionQuery.data?.status === "active" || subscriptionQuery.data?.status === "trialing"
    if (!active && noSubscription) router.replace(`/plans?redirect=${encodeURIComponent(pathname)}`)
  }, [clear, initialized, meQuery.error, meQuery.isLoading, pathname, router, subscriptionQuery.data?.status, subscriptionQuery.error, subscriptionQuery.isError, subscriptionQuery.isLoading, user])

  const isLoading = !initialized && meQuery.isLoading
  const hasActiveSubscription = subscriptionQuery.data?.status === "active" || subscriptionQuery.data?.status === "trialing"
  const accessPending = !initialized || meQuery.isLoading || (user !== null && subscriptionQuery.isLoading)
  const meUnauthorized = meQuery.error instanceof ApiClientError && meQuery.error.status === 401
  const noSubscription = subscriptionQuery.error instanceof ApiClientError && subscriptionQuery.error.status === 404
  const dependencyError = (meQuery.isError && !meUnauthorized) || (subscriptionQuery.isError && !noSubscription)

  if (dependencyError) {
    return (
      <div className="flex min-h-dvh items-center justify-center bg-background px-4">
        <div className="space-y-3 text-center">
          <p className="font-medium">No se pudo verificar el acceso</p>
          <p className="text-sm text-muted-foreground">El servicio no está disponible. Intenta recargar en unos instantes.</p>
        </div>
      </div>
    )
  }

  if (accessPending || meUnauthorized || !user || noSubscription || !hasActiveSubscription) {
    return (
      <div className="flex min-h-dvh items-center justify-center bg-background px-4">
        <div className="space-y-3 text-center">
          <Skeleton className="mx-auto h-8 w-48" />
          <p className="text-sm text-muted-foreground">
            {subscriptionQuery.isError ? "Redirigiendo a la selección de plan..." : "Verificando tu acceso..."}
          </p>
        </div>
      </div>
    )
  }

  async function handleLogout() {
    try {
      await authApi.logout()
    } catch {
      // proceed anyway
    }
    clear()
    router.push("/login")
  }

  const displayName = user?.display_name ?? user?.email ?? "..."

  return (
    <div className="flex min-h-dvh bg-background">
      {/* Mobile overlay */}
      {sidebarOpen && (
        <div
          className="fixed inset-0 z-40 bg-black/40 lg:hidden"
          onClick={() => setSidebarOpen(false)}
        />
      )}

      {/* Sidebar */}
      <aside
        id="sidebar"
        className={cn(
          "fixed inset-y-0 left-0 z-50 flex w-64 flex-col border-r border-border bg-card transition-transform duration-200 lg:static lg:translate-x-0",
          sidebarOpen ? "translate-x-0" : "-translate-x-full",
        )}
        aria-label="Menú lateral"
      >
        <div className="flex h-16 items-center justify-between border-b border-border px-4">
          <Link href="/dashboard" className="flex items-center gap-2 font-semibold tracking-tight">
            <BrandMark />
            <span>Nexus</span>
          </Link>
          <button onClick={() => setSidebarOpen(false)} className="lg:hidden" aria-label="Cerrar menú lateral">
            <X className="size-5" />
          </button>
        </div>
        <nav className="flex-1 space-y-1 overflow-y-auto p-3" aria-label="Navegación principal">
          {NAV_ITEMS.map((item) => {
            const Icon = item.icon
            const active = pathname === item.href || pathname.startsWith(item.href + "/")
            return (
              <Link
                key={item.href}
                href={item.href}
                onClick={() => setSidebarOpen(false)}
                aria-current={active ? "page" : undefined}
                className={cn(
                  "flex items-center gap-3 rounded-lg px-3 py-2 text-sm font-medium transition-colors",
                  active
                    ? "bg-accent text-accent-foreground"
                    : "text-muted-foreground hover:bg-accent/50 hover:text-accent-foreground",
                )}
              >
                <Icon className="size-4 shrink-0" />
                {item.label}
              </Link>
            )
          })}
        </nav>
        <div className="border-t border-border p-3">
          <DropdownMenu>
            <DropdownMenuTrigger className="flex w-full items-center gap-3 rounded-lg px-3 py-2 text-sm transition-colors hover:bg-accent/50">
              {isLoading ? (
                <>
                  <Skeleton className="size-8 rounded-full" />
                  <div className="flex-1 text-left">
                    <Skeleton className="h-3 w-20" />
                    <Skeleton className="mt-1 h-2 w-28" />
                  </div>
                </>
              ) : (
                <>
                  <UserAvatar name={user?.display_name ?? null} email={user?.email ?? ""} />
                  <div className="flex-1 overflow-hidden text-left">
                    <p className="truncate font-medium">{displayName}</p>
                    <p className="truncate text-xs text-muted-foreground">{user?.email}</p>
                  </div>
                  <ChevronDown className="size-4 shrink-0 text-muted-foreground" />
                </>
              )}
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-56">
              <DropdownMenuItem render={<Link href="/profile" />}>
                Perfil
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem onClick={handleLogout}>
                <LogOut className="mr-2 size-4" />
                Cerrar sesión
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </aside>

      {/* Main area */}
      <div className="flex flex-1 flex-col">
        <header className="sticky top-0 z-30 flex h-16 items-center gap-4 border-b border-border bg-background/80 px-4 backdrop-blur lg:px-6">
          <button onClick={() => setSidebarOpen(true)} className="lg:hidden" aria-label="Abrir menú lateral" aria-controls="sidebar" aria-expanded={sidebarOpen}>
            <Menu className="size-5" />
          </button>
          <div className="flex-1" />
          <ThemeToggle />
        </header>
        <a
          href="#dashboard-content"
          className="sr-only focus:not-sr-only focus:absolute focus:z-50 focus:rounded-md focus:bg-background focus:px-4 focus:py-2 focus:text-foreground focus:ring-2 focus:ring-ring"
        >
          Saltar al contenido principal
        </a>
        <main id="dashboard-content" tabIndex={-1} className="flex-1 px-4 py-6 outline-none lg:px-6">
          <ErrorBoundary>{children}</ErrorBoundary>
        </main>
      </div>
    </div>
  )
}
