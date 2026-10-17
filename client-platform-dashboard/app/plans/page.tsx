import type { Metadata } from "next"
import Link from "next/link"
import { BrandLogo } from "@/components/brand"
import { ThemeToggle } from "@/components/theme-toggle"
import { PlansGrid } from "@/components/plans/plans-grid"

export const metadata: Metadata = {
  title: "Planes y precios",
  description: "Elige el plan que se ajusta a tu carga de trabajo.",
}

export default function PlansPage() {
  return (
    <div className="flex min-h-svh flex-col bg-background">
      <header className="sticky top-0 z-40 border-b border-border/60 bg-background/80 backdrop-blur">
        <div className="mx-auto flex h-16 w-full max-w-6xl items-center justify-between px-4">
          <Link href="/" aria-label="Inicio">
            <BrandLogo />
          </Link>
          <div className="flex items-center gap-2">
            <ThemeToggle />
          </div>
        </div>
      </header>
      <main className="mx-auto w-full max-w-6xl flex-1 px-4 py-12">
        <div className="mx-auto max-w-2xl text-center">
          <h1 className="text-balance text-3xl font-semibold tracking-tight sm:text-4xl">
            Elige tu plan
          </h1>
          <p className="mt-3 text-pretty text-muted-foreground">
            Selecciona un plan para activar tu cuenta. Puedes cambiarlo o cancelarlo en cualquier momento.
          </p>
        </div>
        <div className="mt-12">
          <PlansGrid />
        </div>
      </main>
    </div>
  )
}
