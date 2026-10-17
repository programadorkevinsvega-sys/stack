"use client"

import { useState } from "react"
import Link from "next/link"
import { useQuery } from "@tanstack/react-query"
import { invoicesApi } from "@/lib/api/invoices"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Badge } from "@/components/ui/badge"
import { Skeleton } from "@/components/ui/skeleton"

const statusLabels: Record<string, { label: string; variant: "default" | "secondary" | "outline" | "destructive" }> = {
  paid: { label: "Pagada", variant: "secondary" },
  open: { label: "Pendiente", variant: "default" },
  void: { label: "Anulada", variant: "outline" },
  uncollectible: { label: "Incobrable", variant: "destructive" },
}

export default function InvoicesPage() {
  const [statusFilter, setStatusFilter] = useState<string | undefined>()

  const { data, isLoading } = useQuery({
    queryKey: ["admin", "invoices", statusFilter],
    queryFn: () => invoicesApi.list({ status: statusFilter }),
  })

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">Facturas</h1>
        <p className="mt-1 text-sm text-muted-foreground">Gestiona las facturas del sistema.</p>
      </div>

      <div className="flex gap-2">
        {["paid", "open", "void", "uncollectible"].map((s) => {
          const st = statusLabels[s] ?? { label: s, variant: "outline" as const }
          return (
            <Button
              key={s}
              variant={statusFilter === s ? "default" : "outline"}
              size="sm"
              onClick={() => setStatusFilter(statusFilter === s ? undefined : s)}
            >
              {st.label}
            </Button>
          )
        })}
      </div>

      <Card>
        <CardHeader>
          <CardTitle className="text-base">{data ? `${data.length} facturas` : "Facturas"}</CardTitle>
        </CardHeader>
        <CardContent className="p-0">
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-border text-left text-muted-foreground">
                  <th className="px-4 py-3 font-medium">Usuario</th>
                  <th className="px-4 py-3 font-medium">Monto</th>
                  <th className="px-4 py-3 font-medium">Estado</th>
                  <th className="px-4 py-3 font-medium">Vencimiento</th>
                  <th className="px-4 py-3 font-medium">Pagada</th>
                  <th className="px-4 py-3 font-medium" />
                </tr>
              </thead>
              <tbody>
                {isLoading ? (
                  Array.from({ length: 5 }).map((_, i) => (
                    <tr key={i} className="border-b border-border">
                      {Array.from({ length: 6 }).map((_, j) => (
                        <td key={j} className="px-4 py-3"><Skeleton className="h-4 w-20" /></td>
                      ))}
                    </tr>
                  ))
                ) : data?.length === 0 ? (
                  <tr>
                    <td colSpan={6} className="px-4 py-8 text-center text-sm text-muted-foreground">
                      No hay facturas.
                    </td>
                  </tr>
                ) : (
                  data?.map((inv) => {
                    const st = statusLabels[inv.status] ?? { label: inv.status, variant: "outline" as const }
                    return (
                      <tr key={inv.id} className="border-b border-border transition-colors hover:bg-accent/30">
                        <td className="px-4 py-3">{inv.user_email}</td>
                        <td className="px-4 py-3 font-medium">${inv.amount.toFixed(2)}</td>
                        <td className="px-4 py-3"><Badge variant={st.variant}>{st.label}</Badge></td>
                        <td className="px-4 py-3 text-muted-foreground">{new Date(inv.due_date).toLocaleDateString()}</td>
                        <td className="px-4 py-3 text-muted-foreground">
                          {inv.paid_at ? new Date(inv.paid_at).toLocaleDateString() : "—"}
                        </td>
                        <td className="px-4 py-3">
                          <Button variant="ghost" size="sm" render={<Link href={`/invoices/${inv.id}`} />}>
                            Ver
                          </Button>
                        </td>
                      </tr>
                    )
                  })
                )}
              </tbody>
            </table>
          </div>
        </CardContent>
      </Card>
    </div>
  )
}
