"use client"

import { create } from "zustand"
import type { User } from "@/lib/types"
import { setRefreshToken } from "@/lib/api/client"

interface SessionState {
  user: User | null
  // Whether we've attempted to hydrate the current user from the BFF yet.
  initialized: boolean
  setSession: (user: User, refreshToken?: string | null) => void
  setUser: (user: User | null) => void
  setInitialized: (v: boolean) => void
  clear: () => void
}

// Note: nothing sensitive is persisted. The refresh token is kept only in the
// api client's module-scoped memory; the JWT session lives in an HttpOnly cookie.
export const useSession = create<SessionState>((set) => ({
  user: null,
  initialized: false,
  setSession: (user, refreshToken) => {
    if (refreshToken !== undefined) setRefreshToken(refreshToken ?? null)
    set({ user, initialized: true })
  },
  setUser: (user) => set({ user }),
  setInitialized: (v) => set({ initialized: v }),
  clear: () => {
    setRefreshToken(null)
    set({ user: null, initialized: false })
  },
}))
