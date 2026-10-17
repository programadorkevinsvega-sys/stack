// Central runtime configuration.
// The BFF base URL is always read from the env var — never hardcode URLs elsewhere.

export const BFF_URL = process.env.NEXT_PUBLIC_BFF_URL ?? ""

// The request function in client.ts validates that BFF_URL is present before
// making any fetch call. This allows static page generation during build to
// proceed without env vars.
export const API_BASE = BFF_URL
