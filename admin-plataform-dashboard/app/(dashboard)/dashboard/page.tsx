"use client"

import { useQuery } from "@tanstack/react-query"
import { Users, Boxes, CreditCard, Activity, DollarSign, AlertTriangle } from "lucide-react"
import { usageApi } from "@/lib/api/usage"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Skeleton } from "@/components/ui/skeleton"

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

export default function AdminDashboardPage() {
  const summary = useQuery({ queryKey: ["admin", "summary"], queryFn: usageApi.summary })

  const d = summary.data

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">Panel de administración</h1>
        <p className="mt-1 text-sm text-muted-foreground">Resumen del sistema Nexus.</p>
      </div>

      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-6">
        <StatCard
          icon={Users}
          label="Usuarios"
          value={d ? d.total_users.toLocaleString() : <Skeleton className="h-7 w-16" />}
        />
        <StatCard
          icon={Activity}
          label="Requests (30d)"
          value={d ? d.total_requests_30d.toLocaleString() : <Skeleton className="h-7 w-20" />}
        />
        <StatCard
          icon={Boxes}
          label="Tokens (30d)"
          value={d ? d.total_tokens_30d.toLocaleString() : <Skeleton className="h-7 w-20" />}
        />
        <StatCard
          icon={DollarSign}
          label="Ingresos (30d)"
          value={d ? `$${d.total_revenue_30d.toLocaleString()}` : <Skeleton className="h-7 w-16" />}
        />
        <StatCard
          icon={CreditCard}
          label="Suscripciones activas"
          value={d ? d.active_subscriptions.toLocaleString() : <Skeleton className="h-7 w-10" />}
        />
        <StatCard
          icon={AlertTriangle}
          label="Tasa de error"
          value={d ? `${d.error_rate_30d}%` : <Skeleton className="h-7 w-12" />}
        />
      </div>
    </div>
  )
}
