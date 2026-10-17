"use client"

import { create } from "zustand"
import type { AdminUser } from "@/lib/types"
import { setRefreshToken } from "@/lib/api/client"

interface SessionState {
  user: AdminUser | null
  initialized: boolean
  setSession: (user: AdminUser, refreshToken?: string | null) => void
  setUser: (user: AdminUser | null) => void
  setInitialized: (v: boolean) => void
  clear: () => void
}

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
    set({ user: null })
  },
}))
