"use client"

import { useQuery } from "@tanstack/react-query"
import {
  BarChart3,
  KeyRound,
  Wallet,
  Activity,
  TrendingUp,
  AlertTriangle,
  Zap,
  Clock,
  ArrowUpRight,
} from "lucide-react"
import { usageApi } from "@/lib/api/usage"
import { subscriptionApi } from "@/lib/api/subscription"
import { walletApi } from "@/lib/api/wallet"
import { apiKeysApi } from "@/lib/api/apiKeys"
import { invoicesApi } from "@/lib/api/invoices"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Skeleton } from "@/components/ui/skeleton"
import { Badge } from "@/components/ui/badge"

function StatCard({
  icon: Icon,
  label,
  value,
  sub,
}: {
  icon: typeof Activity
  label: string
  value: React.ReactNode
  sub?: string
}) {
  return (
    <Card>
      <CardHeader className="flex flex-row items-center justify-between pb-2">
        <CardTitle className="text-sm font-medium text-muted-foreground">{label}</CardTitle>
        <Icon className="size-4 text-muted-foreground" />
      </CardHeader>
      <CardContent>
        <div className="text-2xl font-bold tracking-tight">{value}</div>
        {sub && <p className="mt-1 text-xs text-muted-foreground">{sub}</p>}
      </CardContent>
    </Card>
  )
}

export default function DashboardPage() {
  const subscription = useQuery({ queryKey: ["subscription"], queryFn: subscriptionApi.get, retry: false })
  const hasDashboardAccess = subscription.data?.status === "active" || subscription.data?.status === "trialing"
  const usage = useQuery({ queryKey: ["usage"], queryFn: usageApi.summary, enabled: hasDashboardAccess })
  const topModels = useQuery({ queryKey: ["usage", "top-models"], queryFn: usageApi.topModels, enabled: hasDashboardAccess })
  const latency = useQuery({ queryKey: ["usage", "latency"], queryFn: usageApi.latency, enabled: hasDashboardAccess })
  const wallet = useQuery({ queryKey: ["wallet"], queryFn: walletApi.get, enabled: hasDashboardAccess })
  const keys = useQuery({ queryKey: ["api-keys"], queryFn: apiKeysApi.list, enabled: hasDashboardAccess })
  const invoices = useQuery({ queryKey: ["invoices"], queryFn: invoicesApi.list, enabled: hasDashboardAccess })

  if (subscription.isLoading) {
    return <div className="space-y-6"><h1 className="text-2xl font-semibold tracking-tight">Panel</h1><Skeleton className="h-40 w-full" /></div>
  }

  if (!hasDashboardAccess) {
    return <div className="space-y-6"><h1 className="text-2xl font-semibold tracking-tight">Panel</h1><Card><CardHeader><CardTitle>Activa un plan para entrar al panel</CardTitle></CardHeader><CardContent><a className="inline-flex rounded-md bg-primary px-4 py-2 text-sm text-primary-foreground" href="/plans">Ver planes</a></CardContent></Card></div>
  }

  const activeKeys = keys.data?.filter((k) => k.status === "active").length ?? 0
  const pendingInvoices = invoices.data?.filter((i) => i.status === "open").length ?? 0
  const lastInvoice = invoices.data?.[0]

  const d = usage.data

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">Panel</h1>
        <p className="mt-1 text-sm text-muted-foreground">Resumen de tu cuenta y uso.</p>
      </div>

      {/* Stat cards */}
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <StatCard
          icon={BarChart3}
          label="Requests totales"
          value={d ? d.requests.toLocaleString() : <Skeleton className="h-7 w-20" />}
          sub={d ? `Tokens: ${(d.tokens_in + d.tokens_out).toLocaleString()}` : undefined}
        />
        <StatCard
          icon={Activity}
          label="Tasa de error"
          value={d ? `${d.error_rate}%` : <Skeleton className="h-7 w-16" />}
          sub={d ? `${d.error_count} errores` : undefined}
        />
        <StatCard
          icon={Clock}
          label="Latencia promedio"
          value={latency.data ? `${latency.data.avg}ms` : <Skeleton className="h-7 w-16" />}
          sub={latency.data ? `p95: ${latency.data.p95}ms` : undefined}
        />
        <StatCard
          icon={Wallet}
          label="Saldo en billetera"
          value={wallet.data ? `$${wallet.data.balance.toFixed(2)}` : <Skeleton className="h-7 w-16" />}
          sub={wallet.data ? `${wallet.data.currency}` : undefined}
        />
      </div>

      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
        {/* Active API keys */}
        <Card>
          <CardHeader className="flex flex-row items-center justify-between pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">API Keys activas</CardTitle>
            <KeyRound className="size-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold tracking-tight">
              {keys.isLoading ? <Skeleton className="h-7 w-10" /> : activeKeys}
            </div>
            <p className="mt-1 text-xs text-muted-foreground">
              {keys.data?.length ? `${keys.data.length} total, ${activeKeys} activas` : "Sin keys"}
            </p>
          </CardContent>
        </Card>

        {/* Pending invoices */}
        <Card>
          <CardHeader className="flex flex-row items-center justify-between pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">Facturas pendientes</CardTitle>
            <AlertTriangle className="size-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold tracking-tight">
              {invoices.isLoading ? <Skeleton className="h-7 w-10" /> : pendingInvoices}
            </div>
            {lastInvoice && (
              <p className="mt-1 text-xs text-muted-foreground">
                Última factura: ${lastInvoice.amount.toFixed(2)} —{" "}
                <Badge variant={lastInvoice.status === "paid" ? "secondary" : "default"} className="text-[10px]">
                  {lastInvoice.status === "paid" ? "Pagada" : lastInvoice.status === "open" ? "Pendiente" : lastInvoice.status}
                </Badge>
              </p>
            )}
          </CardContent>
        </Card>

        {/* Top model */}
        <Card>
          <CardHeader className="flex flex-row items-center justify-between pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">Modelo más usado</CardTitle>
            <TrendingUp className="size-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            {topModels.data?.[0] ? (
              <>
                <div className="text-2xl font-bold tracking-tight">{topModels.data[0].display_name}</div>
                <p className="mt-1 text-xs text-muted-foreground">
                  {topModels.data[0].requests.toLocaleString()} requests
                </p>
              </>
            ) : topModels.isLoading ? (
              <Skeleton className="h-7 w-24" />
            ) : (
              <p className="text-sm text-muted-foreground">Sin datos</p>
            )}
          </CardContent>
        </Card>
      </div>

      {/* Daily usage chart */}
      <Card>
        <CardHeader>
          <CardTitle className="text-base">Uso diario (últimos 30 días)</CardTitle>
        </CardHeader>
        <CardContent>
          {d?.series ? (
            <div className="h-48">
              <div className="flex h-full items-end gap-[3px]">
                {d.series.map((s, i) => {
                  const max = Math.max(...d.series.map((x) => x.requests), 1)
                  const h = (s.requests / max) * 100
                  return (
                    <div
                      key={s.date}
                      className="relative flex flex-1 items-end"
                      title={`${s.date}: ${s.requests.toLocaleString()} req`}
                    >
                      <div
                        className="w-full rounded-t bg-primary/60 transition-all hover:bg-primary"
                        style={{ height: `${Math.max(h, 1)}%` }}
                      />
                    </div>
                  )
                })}
              </div>
            </div>
          ) : (
            <Skeleton className="h-48 w-full" />
          )}
          <div className="mt-2 flex items-center justify-between text-xs text-muted-foreground">
            <span>Hace 30 días</span>
            <span>Hoy</span>
          </div>
        </CardContent>
      </Card>
    </div>
  )
}
