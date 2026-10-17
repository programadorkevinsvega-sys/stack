"use client"

import { useEffect, useRef } from "react"
import { ErrorBoundary } from "@/components/error-boundary"
import { useRouter } from "next/navigation"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { ThemeProvider } from "next-themes"
import { Toaster } from "@/components/ui/sonner"
import { setUnauthorizedHandler } from "@/lib/api/client"
import { useSession } from "@/lib/store/session"

function makeQueryClient() {
  return new QueryClient({
    defaultOptions: {
      queries: {
        retry: 1,
        refetchOnWindowFocus: false,
        staleTime: 30_000,
      },
    },
  })
}

export function Providers({ children }: { children: React.ReactNode }) {
  const router = useRouter()
  const clear = useSession((s) => s.clear)
  const qc = useRef<QueryClient>(undefined)
  if (!qc.current) qc.current = makeQueryClient()

  useEffect(() => {
    setUnauthorizedHandler(() => {
      clear()
      if (typeof window !== "undefined" && !window.location.pathname.startsWith("/login")) {
        router.replace("/login")
      }
    })
  }, [clear, router])

  return (
    <ThemeProvider attribute="class" defaultTheme="dark" enableSystem={false}>
      <QueryClientProvider client={qc.current}>
        <ErrorBoundary>{children}</ErrorBoundary>
        <Toaster position="top-right" richColors />
      </QueryClientProvider>
    </ThemeProvider>
  )
}
