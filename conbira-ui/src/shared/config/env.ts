import { z } from 'zod'

export const env = z
  .object({
    apiBaseUrl: z.url({ protocol: /^https?$/ }),
  })
  .parse({
    apiBaseUrl: import.meta.env.VITE_API_BASE_URL ?? globalThis.location.origin,
  })
