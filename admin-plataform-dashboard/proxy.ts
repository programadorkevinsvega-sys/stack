import { NextResponse, type NextRequest } from "next/server"

const PROTECTED_PREFIXES = [
  "/dashboard",
  "/users",
  "/models",
  "/plans",
  "/wallets",
  "/invoices",
  "/settings",
  "/storage",
]

export async function proxy(req: NextRequest) {
  const { pathname } = req.nextUrl
  const isProtected = PROTECTED_PREFIXES.some(
    (p) => pathname === p || pathname.startsWith(`${p}/`),
  )
  if (!isProtected) return NextResponse.next()

  const bffUrl = process.env.NEXT_PUBLIC_ADMIN_BFF_URL
  if (!bffUrl) {
    const url = req.nextUrl.clone()
    url.pathname = "/login"
    url.searchParams.set("redirect", pathname)
    return NextResponse.redirect(url)
  }

  const sessionToken = req.cookies.get("session_token")?.value
  if (!sessionToken) {
    const url = req.nextUrl.clone()
    url.pathname = "/login"
    url.searchParams.set("redirect", pathname)
    return NextResponse.redirect(url)
  }

  try {
    const validateRes = await fetch(`${bffUrl}/api/v1/me`, {
      headers: { Cookie: req.headers.get("cookie") ?? "" },
    })

    if (validateRes.ok) {
      const setCookie = validateRes.headers.get("set-cookie")
      if (setCookie) {
        const response = NextResponse.next()
        response.headers.set("set-cookie", setCookie)
        return response
      }
      return NextResponse.next()
    }

    const url = req.nextUrl.clone()
    url.pathname = "/login"
    url.searchParams.set("redirect", pathname)
    return NextResponse.redirect(url)
  } catch {
    return NextResponse.next()
  }
}

export const config = {
  matcher: [
    "/dashboard/:path*",
    "/users/:path*",
    "/models/:path*",
    "/plans/:path*",
    "/wallets/:path*",
    "/invoices/:path*",
    "/settings/:path*",
    "/storage/:path*",
  ],
}
