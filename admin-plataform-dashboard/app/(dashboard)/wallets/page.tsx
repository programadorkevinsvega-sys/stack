"use client"

import { useState } from "react"
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query"
import { useForm } from "react-hook-form"
import { zodResolver } from "@hookform/resolvers/zod"
import { toast } from "sonner"
import { Search, Wallet } from "lucide-react"
import { walletsApi } from "@/lib/api/wallets"
import { adjustWalletSchema, type AdjustWalletValues } from "@/lib/validation"
import { Input } from "@/components/ui/input"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Label } from "@/components/ui/label"
import { Skeleton } from "@/components/ui/skeleton"

export default function WalletsPage() {
  const qc = useQueryClient()
  const [userId, setUserId] = useState("")

  const { data: wallet, isLoading, isError } = useQuery({
    queryKey: ["admin", "wallets", userId],
    queryFn: () => walletsApi.get(userId),
    enabled: userId.length > 0,
    retry: false,
  })

  const { register, handleSubmit, reset, formState: { errors } } = useForm<AdjustWalletValues>({
    resolver: zodResolver(adjustWalletSchema),
  })

  const adjustMutation = useMutation({
    mutationFn: (values: AdjustWalletValues) => walletsApi.adjust(userId, values),
    onSuccess: (data) => {
      qc.setQueryData(["admin", "wallets", userId], data)
      toast.success("Saldo ajustado")
      reset()
    },
    onError: (e) => toast.error(e instanceof Error ? e.message : "Error"),
  })

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">Billeteras</h1>
        <p className="mt-1 text-sm text-muted-foreground">Consulta y ajusta saldos de usuarios.</p>
      </div>

      <div className="flex gap-3">
        <div className="relative flex-1">
          <Search className="absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
          <Input
            placeholder="ID del usuario..."
            value={userId}
            onChange={(e) => setUserId(e.target.value)}
            className="pl-9"
          />
        </div>
      </div>

      {userId && (
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2 text-base">
              <Wallet className="size-4" />
              {wallet ? `Billetera de ${wallet.user_email}` : "Billetera"}
            </CardTitle>
          </CardHeader>
          <CardContent>
            {isLoading ? (
              <Skeleton className="h-20 w-full" />
            ) : isError ? (
              <p className="text-sm text-destructive">No se encontró la billetera para este usuario.</p>
            ) : wallet ? (
              <div className="space-y-6">
                <div className="grid gap-4 sm:grid-cols-3">
                  <div>
                    <p className="text-sm text-muted-foreground">Saldo actual</p>
                    <p className="text-2xl font-bold">${wallet.balance.toFixed(2)}</p>
                  </div>
                  <div>
                    <p className="text-sm text-muted-foreground">Moneda</p>
                    <p className="text-2xl font-bold">{wallet.currency}</p>
                  </div>
                  <div>
                    <p className="text-sm text-muted-foreground">Límite de crédito</p>
                    <p className="text-2xl font-bold">${wallet.credit_limit.toFixed(2)}</p>
                  </div>
                </div>

                <form onSubmit={handleSubmit((values) => adjustMutation.mutate(values))} className="space-y-4 border-t border-border pt-4">
                  <p className="font-medium">Ajustar saldo</p>
                  <div className="grid gap-4 sm:grid-cols-2">
                    <div className="flex flex-col gap-2">
                      <Label htmlFor="amount">Monto (positivo = abono, negativo = cargo)</Label>
                      <Input id="amount" type="number" step="0.01" {...register("amount", { valueAsNumber: true })} aria-invalid={!!errors.amount} />
                    </div>
                    <div className="flex flex-col gap-2">
                      <Label htmlFor="reason">Razón</Label>
                      <Input id="reason" {...register("reason")} aria-invalid={!!errors.reason} />
                    </div>
                  </div>
                  <Button type="submit" disabled={adjustMutation.isPending}>
                    {adjustMutation.isPending ? "Ajustando..." : "Ajustar saldo"}
                  </Button>
                </form>
              </div>
            ) : null}
          </CardContent>
        </Card>
      )}
    </div>
  )
}
