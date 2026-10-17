"use client"

import { useState } from "react"
import { useQuery } from "@tanstack/react-query"
import { Receipt, ExternalLink, Search } from "lucide-react"
import { invoicesApi } from "@/lib/api/invoices"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Badge } from "@/components/ui/badge"
import { Skeleton } from "@/components/ui/skeleton"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog"
import type { Invoice } from "@/lib/types"

const statusLabel: Record<string, string> = {
  paid: "Pagada",
  open: "Pendiente",
  void: "Anulada",
  uncollectible: "Incobrable",
}

const statusVariant: Record<string, "default" | "secondary" | "outline" | "destructive"> = {
  paid: "default",
  open: "secondary",
  void: "outline",
  uncollectible: "destructive",
}

function InvoiceDetail({ invoice }: { invoice: Invoice }) {
  return (
    <div className="space-y-4">
      <div className="grid grid-cols-2 gap-3">
        <div>
          <p className="text-xs text-muted-foreground">ID</p>
          <p className="font-mono text-sm">{invoice.id}</p>
        </div>
        <div>
          <p className="text-xs text-muted-foreground">Estado</p>
          <Badge variant={statusVariant[invoice.status] ?? "outline"}>
            {statusLabel[invoice.status] ?? invoice.status}
          </Badge>
        </div>
        <div>
          <p className="text-xs text-muted-foreground">Monto</p>
          <p className="font-medium">${invoice.amount.toFixed(2)} {invoice.currency}</p>
        </div>
        <div>
          <p className="text-xs text-muted-foreground">Vencimiento</p>
          <p className="text-sm">{new Date(invoice.due_date).toLocaleDateString()}</p>
        </div>
        {invoice.paid_at && (
          <div>
            <p className="text-xs text-muted-foreground">Pagada</p>
            <p className="text-sm">{new Date(invoice.paid_at).toLocaleDateString()}</p>
          </div>
        )}
      </div>
      {invoice.line_items && invoice.line_items.length > 0 && (
        <div>
          <p className="mb-2 text-sm font-medium">Items</p>
          <div className="space-y-1">
            {invoice.line_items.map((item, i) => (
              <div key={i} className="flex items-center justify-between rounded-lg bg-muted/30 px-3 py-2 text-sm">
                <span>{item.description}</span>
                <span className="font-medium">${item.amount.toFixed(2)}</span>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  )
}

export default function InvoicesPage() {
  const { data: invoices, isLoading } = useQuery({
    queryKey: ["invoices"],
    queryFn: invoicesApi.list,
  })
  const [search, setSearch] = useState("")
  const [selected, setSelected] = useState<Invoice | null>(null)

  const filtered = invoices?.filter(
    (inv) =>
      inv.id.toLowerCase().includes(search.toLowerCase()) ||
      inv.status.toLowerCase().includes(search.toLowerCase()),
  )

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">Facturas</h1>
        <p className="mt-1 text-sm text-muted-foreground">
          Historial de facturación y recibos.
        </p>
      </div>

      <div className="relative">
        <Search className="absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
        <Input
          placeholder="Buscar por ID o estado..."
          className="pl-9"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
      </div>

      <Card>
        <CardContent className="p-0">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Factura</TableHead>
                <TableHead>Monto</TableHead>
                <TableHead>Estado</TableHead>
                <TableHead>Vencimiento</TableHead>
                <TableHead>Pagada</TableHead>
                <TableHead className="w-12" />
              </TableRow>
            </TableHeader>
            <TableBody>
              {isLoading ? (
                Array.from({ length: 3 }).map((_, i) => (
                  <TableRow key={i}>
                    <TableCell><Skeleton className="h-4 w-24" /></TableCell>
                    <TableCell><Skeleton className="h-4 w-16" /></TableCell>
                    <TableCell><Skeleton className="h-5 w-16" /></TableCell>
                    <TableCell><Skeleton className="h-4 w-24" /></TableCell>
                    <TableCell><Skeleton className="h-4 w-24" /></TableCell>
                    <TableCell />
                  </TableRow>
                ))
              ) : filtered?.length ? (
                filtered.map((inv) => (
                  <TableRow key={inv.id}>
                    <TableCell className="font-mono text-xs">{inv.id}</TableCell>
                    <TableCell className="font-medium">
                      ${inv.amount.toFixed(2)} {inv.currency}
                    </TableCell>
                    <TableCell>
                      <Badge variant={statusVariant[inv.status] ?? "outline"}>
                        {statusLabel[inv.status] ?? inv.status}
                      </Badge>
                    </TableCell>
                    <TableCell className="text-sm text-muted-foreground">
                      {new Date(inv.due_date).toLocaleDateString()}
                    </TableCell>
                    <TableCell className="text-sm text-muted-foreground">
                      {inv.paid_at ? new Date(inv.paid_at).toLocaleDateString() : "—"}
                    </TableCell>
                    <TableCell>
                      <Dialog>
                        <DialogTrigger render={<Button variant="ghost" size="icon" onClick={() => setSelected(inv)}><ExternalLink className="size-4" /></Button>} />
                        <DialogContent>
                          <DialogHeader>
                            <DialogTitle>Detalle de factura</DialogTitle>
                          </DialogHeader>
                          <InvoiceDetail invoice={selected ?? inv} />
                        </DialogContent>
                      </Dialog>
                    </TableCell>
                  </TableRow>
                ))
              ) : (
                <TableRow>
                  <TableCell colSpan={6} className="py-12 text-center text-sm text-muted-foreground">
                    <Receipt className="mx-auto mb-2 size-8 text-muted-foreground/50" />
                    {search ? "No se encontraron facturas" : "Sin facturas aún"}
                  </TableCell>
                </TableRow>
              )}
            </TableBody>
          </Table>
        </CardContent>
      </Card>
    </div>
  )
}
