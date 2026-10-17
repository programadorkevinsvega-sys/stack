import Link from "next/link"
import { ArrowRight, BarChart3, KeyRound, Layers, ShieldCheck, Zap } from "lucide-react"
import { BrandLogo } from "@/components/brand"
import { ThemeToggle } from "@/components/theme-toggle"
import { Button } from "@/components/ui/button"
import { PricingTeaser } from "@/components/landing/pricing-teaser"

const features = [
  {
    icon: KeyRound,
    title: "API keys en segundos",
    desc: "Crea, rota y revoca claves con permisos granulares. Tus secretos se muestran una sola vez.",
  },
  {
    icon: Layers,
    title: "Catálogo de modelos",
    desc: "Accede a modelos de chat, razonamiento y visión con precios transparentes por token.",
  },
  {
    icon: BarChart3,
    title: "Analíticas de uso",
    desc: "Monitorea requests, tokens, latencia p50/p95/p99 y tasa de error en tiempo real.",
  },
  {
    icon: ShieldCheck,
    title: "Seguridad primero",
    desc: "Sesiones con cookies HttpOnly, protección CSRF y MFA opcional para tu cuenta.",
  },
]

export default function LandingPage() {
  return (
    <div className="flex min-h-dvh flex-col">
      <header className="sticky top-0 z-40 border-b border-border/60 bg-background/80 backdrop-blur">
        <div className="mx-auto flex h-16 w-full max-w-6xl items-center justify-between px-4">
          <BrandLogo />
          <nav className="flex items-center gap-2">
            <ThemeToggle />
            <Button variant="ghost" render={<Link href="/login" />}>
              Iniciar sesión
            </Button>
            <Button render={<Link href="/register" />}>
              Comenzar
            </Button>
          </nav>
        </div>
      </header>

      <main className="flex-1">
        {/* Hero */}
        <section className="relative overflow-hidden border-b border-border/60">
          <div className="mx-auto grid w-full max-w-6xl gap-10 px-4 py-20 lg:grid-cols-2 lg:items-center">
            <div className="flex flex-col gap-6">
              <span className="inline-flex w-fit items-center gap-2 rounded-full border border-border bg-card px-3 py-1 text-xs font-medium text-muted-foreground">
                <Zap className="size-3 text-primary" />
                LLM-as-a-Service para desarrolladores
              </span>
              <h1 className="text-balance text-4xl font-semibold tracking-tight sm:text-5xl lg:text-6xl">
                La plataforma para construir con modelos de lenguaje
              </h1>
              <p className="text-pretty text-lg leading-relaxed text-muted-foreground">
                Una consola limpia para gestionar tus API keys, monitorear el uso, controlar costos
                y desplegar a producción. Sin fricción, con la seguridad que tu equipo necesita.
              </p>
              <div className="flex flex-wrap items-center gap-3">
                <Button size="lg" render={<Link href="/register" />}>
                  Crear cuenta gratis
                  <ArrowRight className="size-4" />
                </Button>
                <Button size="lg" variant="outline" render={<Link href="/plans" />}>
                  Ver planes
                </Button>
              </div>
            </div>

            {/* Console mock */}
            <div className="rounded-xl border border-border bg-card p-1 shadow-2xl shadow-primary/5">
              <div className="flex items-center gap-1.5 px-3 py-2">
                <span className="size-2.5 rounded-full bg-destructive/70" />
                <span className="size-2.5 rounded-full bg-chart-5/70" />
                <span className="size-2.5 rounded-full bg-primary/70" />
              </div>
              <div className="rounded-lg bg-background p-4 font-mono text-xs leading-relaxed">
                <p className="text-muted-foreground">$ curl https://api.nexus.ai/v1/chat \</p>
                <p className="pl-4 text-muted-foreground">
                  -H <span className="text-primary">{'"Authorization: Bearer sk-nx-..."'}</span> \
                </p>
                <p className="pl-4 text-muted-foreground">
                  -d <span className="text-foreground">{'{"model":"nexus-pro"}'}</span>
                </p>
                <p className="mt-3 text-foreground">{"{"}</p>
                <p className="pl-4 text-foreground">
                  {'"id"'}: <span className="text-primary">{'"cmpl_9x2a"'}</span>,
                </p>
                <p className="pl-4 text-foreground">
                  {'"status"'}: <span className="text-chart-3">{'"ok"'}</span>,
                </p>
                <p className="pl-4 text-foreground">
                  {'"latency_ms"'}: <span className="text-chart-2">312</span>
                </p>
                <p className="text-foreground">{"}"}</p>
              </div>
            </div>
          </div>
        </section>

        {/* Features */}
        <section className="mx-auto w-full max-w-6xl px-4 py-20">
          <div className="mx-auto max-w-2xl text-center">
            <h2 className="text-balance text-3xl font-semibold tracking-tight">
              Todo lo que necesitas para enviar a producción
            </h2>
            <p className="mt-3 text-pretty text-muted-foreground">
              Herramientas de desarrollador diseñadas para velocidad, visibilidad y control.
            </p>
          </div>
          <div className="mt-12 grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
            {features.map((f) => (
              <div key={f.title} className="rounded-xl border border-border bg-card p-6">
                <div className="flex size-10 items-center justify-center rounded-lg bg-accent text-primary">
                  <f.icon className="size-5" />
                </div>
                <h3 className="mt-4 font-medium">{f.title}</h3>
                <p className="mt-2 text-sm leading-relaxed text-muted-foreground">{f.desc}</p>
              </div>
            ))}
          </div>
        </section>

        {/* Pricing teaser */}
        <section className="border-t border-border/60 bg-card/30">
          <div className="mx-auto w-full max-w-6xl px-4 py-20">
            <div className="mx-auto max-w-2xl text-center">
              <h2 className="text-balance text-3xl font-semibold tracking-tight">
                Precios simples y transparentes
              </h2>
              <p className="mt-3 text-pretty text-muted-foreground">
                Empieza gratis y escala cuando lo necesites. Sin sorpresas.
              </p>
            </div>
            <div className="mt-12">
              <PricingTeaser />
            </div>
          </div>
        </section>
      </main>

      <footer className="border-t border-border/60">
        <div className="mx-auto flex w-full max-w-6xl flex-col items-center justify-between gap-4 px-4 py-8 sm:flex-row">
          <BrandLogo />
          <p className="text-sm text-muted-foreground">
            {"\u00A9"} {new Date().getFullYear()} Nexus. Todos los derechos reservados.
          </p>
        </div>
      </footer>
    </div>
  )
}
