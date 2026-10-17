import { NextResponse, type NextRequest } from "next/server"

const PROTECTED_PREFIXES = [
  "/dashboard",
  "/api-keys",
  "/subscription",
  "/wallet",
  "/payment-methods",
  "/invoices",
  "/models",
  "/profile",
  "/files",
  "/storage",
  "/rag",
]

export async function proxy(req: NextRequest) {
  const { pathname } = req.nextUrl
  const isProtected = PROTECTED_PREFIXES.some(
    (p) => pathname === p || pathname.startsWith(`${p}/`),
  )
  if (!isProtected) return NextResponse.next()

  const sessionToken = req.cookies.get("session_token")?.value
  if (!sessionToken) {
    const url = req.nextUrl.clone()
    url.pathname = "/login"
    url.searchParams.set("redirect", pathname)
    return NextResponse.redirect(url)
  }

  // This middleware is only a navigation guard. The BFF validates the
  // HttpOnly session on every API request, so performing another /me request
  // here would duplicate authentication during every route prefetch.
  return NextResponse.next()
}

export const config = {
  matcher: [
    "/dashboard/:path*",
    "/api-keys/:path*",
    "/subscription/:path*",
    "/wallet/:path*",
    "/payment-methods/:path*",
    "/invoices/:path*",
    "/models/:path*",
    "/profile/:path*",
    "/files/:path*",
    "/storage/:path*",
    "/rag/:path*",
  ],
}
