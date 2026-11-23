import { z } from 'zod'

export type Catalog = 'roles' | 'permissions'

export const catalogResponse = z.object({
  data: z.array(z.object({
    code: z.string(),
    name: z.string(),
    description: z.string(),
  })),
})
