"use client"

import { useState } from "react"
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query"
import { useRouter } from "next/navigation"
import { toast } from "sonner"
import { Wallet as WalletIcon, Plus, Loader2, DollarSign } from "lucide-react"
import { walletApi } from "@/lib/api/wallet"
import { invoicesApi } from "@/lib/api/invoices"
import { ApiClientError } from "@/lib/api/client"
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
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
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog"
import { Badge } from "@/components/ui/badge"
import { topUpSchema } from "@/lib/validation"

const TOP_UP_AMOUNTS = [10, 25, 50, 100, 250]

export default function WalletPage() {
  const router = useRouter()
  const qc = useQueryClient()
  const [open, setOpen] = useState(false)
  const [amount, setAmount] = useState(50)
  const [customAmount, setCustomAmount] = useState("")
  const [error, setError] = useState("")

  const wallet = useQuery({
    queryKey: ["wallet"],
    queryFn: walletApi.get,
  })
  const invoices = useQuery({
    queryKey: ["invoices"],
    queryFn: invoicesApi.list,
  })

  const topUpMut = useMutation({
    mutationFn: () => {
      const finalAmount = customAmount ? parseFloat(customAmount) : amount
      const parsed = topUpSchema.safeParse({ amount: finalAmount, currency: wallet.data?.currency ?? "USD" })
      if (!parsed.success) {
        setError(parsed.error.issues[0].message)
        throw new Error("validation")
      }
      setError("")
      return walletApi.topUp(finalAmount, wallet.data?.currency ?? "USD", "stripe")
    },
    onSuccess: (data) => {
      router.push(data.redirect_url)
    },
    onError: (e) => {
      if (e instanceof Error && e.message === "validation") return
      toast.error(e instanceof ApiClientError ? e.message : "Error al solicitar recarga")
    },
  })

  function handleOpen() {
    setOpen(true)
    setAmount(50)
    setCustomAmount("")
    setError("")
  }

  const walletData = wallet.data

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">Billetera</h1>
        <p className="mt-1 text-sm text-muted-foreground">
          Saldo disponible y recargas.
        </p>
      </div>

      <div className="grid gap-4 sm:grid-cols-2">
        <Card>
          <CardHeader className="flex flex-row items-center justify-between pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">
              Saldo disponible
            </CardTitle>
            <WalletIcon className="size-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            {wallet.isLoading ? (
              <Skeleton className="h-8 w-24" />
            ) : (
              <>
                <div className="text-3xl font-bold tracking-tight">
                  ${walletData?.balance.toFixed(2)}
                </div>
                <p className="mt-1 text-xs text-muted-foreground">{walletData?.currency}</p>
              </>
            )}
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="flex flex-row items-center justify-between pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">
              Límite de crédito
            </CardTitle>
            <DollarSign className="size-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            {wallet.isLoading ? (
              <Skeleton className="h-8 w-24" />
            ) : (
              <>
                <div className="text-3xl font-bold tracking-tight">
                  ${walletData?.credit_limit.toFixed(2)}
                </div>
                <p className="mt-1 text-xs text-muted-foreground">{walletData?.currency}</p>
              </>
            )}
          </CardContent>
        </Card>
      </div>

      <Dialog open={open} onOpenChange={setOpen}>
        <DialogTrigger render={<Button onClick={handleOpen}><Plus className="mr-2 size-4" />Recargar saldo</Button>} />
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Recargar billetera</DialogTitle>
            <DialogDescription>
              Selecciona un monto o ingresa uno personalizado.
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-4">
            <div className="flex flex-wrap gap-2">
              {TOP_UP_AMOUNTS.map((a) => (
                <Button
                  key={a}
                  variant={amount === a && !customAmount ? "default" : "outline"}
                  size="sm"
                  onClick={() => {
                    setAmount(a)
                    setCustomAmount("")
                    setError("")
                  }}
                >
                  ${a}
                </Button>
              ))}
            </div>
            <div className="flex flex-col gap-2">
              <Label htmlFor="custom-amount">Monto personalizado</Label>
              <Input
                id="custom-amount"
                type="number"
                step="0.01"
                min="1"
                placeholder="p.ej. 75.50"
                value={customAmount}
                onChange={(e) => {
                  setCustomAmount(e.target.value)
                  setError("")
                }}
              />
            </div>
            {error && <p className="text-sm text-destructive">{error}</p>}
            <DialogFooter>
              <Button variant="outline" onClick={() => setOpen(false)}>
                Cancelar
              </Button>
              <Button
                onClick={() => topUpMut.mutate()}
                disabled={topUpMut.isPending}
              >
                {topUpMut.isPending ? (
                  <>
                    <Loader2 className="mr-2 size-4 animate-spin" />
                    Procesando...
                  </>
                ) : (
                  `Recargar ${customAmount ? `$${parseFloat(customAmount).toFixed(2)}` : `$${amount}`}`
                )}
              </Button>
            </DialogFooter>
          </div>
        </DialogContent>
      </Dialog>

      {/* Recent transactions from invoices */}
      <Card>
        <CardHeader>
          <CardTitle className="text-base">Movimientos recientes</CardTitle>
          <CardDescription>Historial de facturación y recargas.</CardDescription>
        </CardHeader>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>ID</TableHead>
                <TableHead>Monto</TableHead>
                <TableHead>Estado</TableHead>
                <TableHead>Fecha</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {invoices.isLoading ? (
                Array.from({ length: 2 }).map((_, i) => (
                  <TableRow key={i}>
                    <TableCell><Skeleton className="h-4 w-20" /></TableCell>
                    <TableCell><Skeleton className="h-4 w-16" /></TableCell>
                    <TableCell><Skeleton className="h-5 w-16" /></TableCell>
                    <TableCell><Skeleton className="h-4 w-24" /></TableCell>
                  </TableRow>
                ))
              ) : invoices.data?.length ? (
                invoices.data.map((inv) => (
                  <TableRow key={inv.id}>
                    <TableCell className="font-mono text-xs">{inv.id}</TableCell>
                    <TableCell>${inv.amount.toFixed(2)}</TableCell>
                    <TableCell>
                      <Badge variant={inv.status === "paid" ? "default" : "secondary"}>
                        {inv.status === "paid" ? "Pagada" : inv.status === "open" ? "Pendiente" : inv.status}
                      </Badge>
                    </TableCell>
                    <TableCell className="text-sm text-muted-foreground">
                      {new Date(inv.due_date).toLocaleDateString()}
                    </TableCell>
                  </TableRow>
                ))
              ) : (
                <TableRow>
                  <TableCell colSpan={4} className="py-8 text-center text-sm text-muted-foreground">
                    Sin movimientos
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
