"use client"

import { use } from "react"
import { useRouter } from "next/navigation"
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query"
import { ArrowLeft, Ban, XCircle } from "lucide-react"
import { toast } from "sonner"
import { invoicesApi } from "@/lib/api/invoices"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Badge } from "@/components/ui/badge"
import { Skeleton } from "@/components/ui/skeleton"
import { Separator } from "@/components/ui/separator"

const statusLabels: Record<string, { label: string; variant: "default" | "secondary" | "outline" | "destructive" }> = {
  paid: { label: "Pagada", variant: "secondary" },
  open: { label: "Pendiente", variant: "default" },
  void: { label: "Anulada", variant: "outline" },
  uncollectible: { label: "Incobrable", variant: "destructive" },
}

export default function InvoiceDetailPage({ params }: { params: Promise<{ id: string }> }) {
  const { id } = use(params)
  const router = useRouter()
  const qc = useQueryClient()

  const { data: invoice, isLoading } = useQuery({
    queryKey: ["admin", "invoices", id],
    queryFn: () => invoicesApi.get(id),
  })

  const voidMutation = useMutation({
    mutationFn: () => invoicesApi.void(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["admin", "invoices"] })
      toast.success("Factura anulada")
    },
    onError: (e) => toast.error(e instanceof Error ? e.message : "Error"),
  })

  const uncollectibleMutation = useMutation({
    mutationFn: () => invoicesApi.markUncollectible(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["admin", "invoices"] })
      toast.success("Factura marcada como incobrable")
    },
    onError: (e) => toast.error(e instanceof Error ? e.message : "Error"),
  })

  if (isLoading) return <Skeleton className="h-64 w-full" />
  if (!invoice) return <p className="text-muted-foreground">Factura no encontrada.</p>

  const st = statusLabels[invoice.status] ?? { label: invoice.status, variant: "outline" as const }

  return (
    <div className="space-y-6">
      <div className="flex items-center gap-4">
        <Button variant="ghost" size="icon" onClick={() => router.back()}>
          <ArrowLeft className="size-4" />
        </Button>
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">Factura #{invoice.id.slice(0, 8)}</h1>
          <p className="text-sm text-muted-foreground">{invoice.user_email}</p>
        </div>
        <Badge variant={st.variant} className="ml-auto">{st.label}</Badge>
      </div>

      <div className="grid gap-4 sm:grid-cols-2">
        <Card>
          <CardHeader><CardTitle className="text-base">Detalles</CardTitle></CardHeader>
          <CardContent className="space-y-3 text-sm">
            <div className="flex justify-between">
              <span className="text-muted-foreground">Monto</span>
              <span className="font-bold">${invoice.amount.toFixed(2)}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-muted-foreground">Moneda</span>
              <span>{invoice.currency}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-muted-foreground">Vencimiento</span>
              <span>{new Date(invoice.due_date).toLocaleDateString()}</span>
            </div>
            {invoice.paid_at && (
              <div className="flex justify-between">
                <span className="text-muted-foreground">Pagada</span>
                <span>{new Date(invoice.paid_at).toLocaleDateString()}</span>
              </div>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader><CardTitle className="text-base">Acciones</CardTitle></CardHeader>
          <CardContent className="space-y-3">
            {(invoice.status === "open" || invoice.status === "uncollectible") && (
              <Button variant="destructive" className="w-full" onClick={() => voidMutation.mutate()} disabled={voidMutation.isPending}>
                <Ban className="mr-2 size-4" />
                Anular factura
              </Button>
            )}
            {invoice.status === "open" && (
              <Button variant="outline" className="w-full" onClick={() => uncollectibleMutation.mutate()} disabled={uncollectibleMutation.isPending}>
                <XCircle className="mr-2 size-4" />
                Marcar como incobrable
              </Button>
            )}
          </CardContent>
        </Card>
      </div>

      {invoice.line_items && invoice.line_items.length > 0 && (
        <Card>
          <CardHeader><CardTitle className="text-base">Line items</CardTitle></CardHeader>
          <CardContent className="p-0">
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b border-border text-left text-muted-foreground">
                    <th className="px-4 py-3 font-medium">Descripción</th>
                    <th className="px-4 py-3 font-medium">Cantidad</th>
                    <th className="px-4 py-3 font-medium">Precio unitario</th>
                    <th className="px-4 py-3 font-medium">Total</th>
                  </tr>
                </thead>
                <tbody>
                  {invoice.line_items.map((item, i) => (
                    <tr key={i} className="border-b border-border">
                      <td className="px-4 py-3">{item.description}</td>
                      <td className="px-4 py-3">{item.quantity}</td>
                      <td className="px-4 py-3">${item.unit_price.toFixed(2)}</td>
                      <td className="px-4 py-3 font-medium">${item.amount.toFixed(2)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  )
}
