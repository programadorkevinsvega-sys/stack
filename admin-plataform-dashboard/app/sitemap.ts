import type { MetadataRoute } from "next"

export default function sitemap(): MetadataRoute.Sitemap {
  return [
    { url: "https://admin.nexus.ai/login", lastModified: new Date(), changeFrequency: "monthly", priority: 0.5 },
    { url: "https://admin.nexus.ai/dashboard", lastModified: new Date(), changeFrequency: "daily", priority: 1 },
  ]
}
