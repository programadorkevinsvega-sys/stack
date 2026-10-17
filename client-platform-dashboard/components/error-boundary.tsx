"use client"

import { Component, createRef } from "react"
import Link from "next/link"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { AlertTriangle, RotateCcw, Home } from "lucide-react"

interface Props {
  children: React.ReactNode
  fallback?: React.ReactNode
}

interface State {
  hasError: boolean
  error: Error | null
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = { hasError: false, error: null }
  private headingRef = createRef<HTMLHeadingElement>()

  static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error }
  }

  componentDidUpdate(_prevProps: Props, prevState: State) {
    if (this.state.hasError && !prevState.hasError) {
      this.headingRef.current?.focus()
    }
  }

  handleReset = () => {
    this.setState({ hasError: false, error: null })
  }

  render() {
    if (this.state.hasError) {
      if (this.props.fallback) return this.props.fallback

      return (
        <div className="flex min-h-dvh items-center justify-center bg-background px-4" role="alert">
          <Card className="w-full max-w-md text-center">
            <CardHeader>
              <div className="mx-auto mb-2 flex size-14 items-center justify-center rounded-full bg-destructive/10">
                <AlertTriangle className="size-8 text-destructive" />
              </div>
              <CardTitle className="text-xl" tabIndex={-1} ref={this.headingRef}>
                Algo salió mal
              </CardTitle>
              <CardDescription>
                Ocurrió un error inesperado. Puedes intentar recargar o volver al inicio.
              </CardDescription>
            </CardHeader>
            <CardContent className="flex flex-col gap-3">
              {this.state.error && (
                <pre className="max-h-24 overflow-auto rounded-lg bg-muted p-3 text-left text-xs text-muted-foreground">
                  {this.state.error.message}
                </pre>
              )}
              <div className="flex flex-col gap-2 sm:flex-row">
                <Button onClick={this.handleReset} className="flex-1">
                  <RotateCcw className="mr-2 size-4" />
                  Reintentar
                </Button>
                <Button variant="outline" render={<Link href="/" />} className="flex-1">
                  <Home className="mr-2 size-4" />
                  Inicio
                </Button>
              </div>
            </CardContent>
          </Card>
        </div>
      )
    }

    return this.props.children
  }
}
