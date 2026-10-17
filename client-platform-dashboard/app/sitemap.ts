import type { MetadataRoute } from "next"

export default function sitemap(): MetadataRoute.Sitemap {
  const base = "https://nexus.ai"
  return [
    { url: base, lastModified: new Date(), changeFrequency: "monthly", priority: 1 },
    { url: `${base}/login`, lastModified: new Date(), changeFrequency: "yearly", priority: 0.3 },
    { url: `${base}/register`, lastModified: new Date(), changeFrequency: "yearly", priority: 0.3 },
    { url: `${base}/plans`, lastModified: new Date(), changeFrequency: "weekly", priority: 0.5 },
    { url: `${base}/dashboard`, lastModified: new Date(), changeFrequency: "weekly", priority: 0.8 },
    { url: `${base}/models`, lastModified: new Date(), changeFrequency: "weekly", priority: 0.6 },
    { url: `${base}/api-keys`, lastModified: new Date(), changeFrequency: "monthly", priority: 0.4 },
    { url: `${base}/subscription`, lastModified: new Date(), changeFrequency: "monthly", priority: 0.4 },
    { url: `${base}/invoices`, lastModified: new Date(), changeFrequency: "monthly", priority: 0.4 },
    { url: `${base}/wallet`, lastModified: new Date(), changeFrequency: "monthly", priority: 0.4 },
    { url: `${base}/payment-methods`, lastModified: new Date(), changeFrequency: "monthly", priority: 0.4 },
    { url: `${base}/profile`, lastModified: new Date(), changeFrequency: "monthly", priority: 0.3 },
    { url: `${base}/storage`, lastModified: new Date(), changeFrequency: "monthly", priority: 0.3 },
    { url: `${base}/rag`, lastModified: new Date(), changeFrequency: "monthly", priority: 0.3 },
  ]
}
