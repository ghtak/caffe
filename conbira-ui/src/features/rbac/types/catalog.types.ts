import { z } from 'zod'

export type Catalog = 'roles' | 'permissions'

export const catalogItemSchema = z.object({
  id: z.number().int(),
  code: z.string(),
  name: z.string(),
  description: z.string(),
})

export const catalogResponse = z.object({ data: z.array(catalogItemSchema) })
