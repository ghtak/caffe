import { queryOptions } from '@tanstack/react-query'
import { z } from 'zod'
import { apiClient } from '@/shared/api/client'

const responseSchema = z.object({ data: z.object({ items: z.array(z.object({
  id: z.number().int(),
  nickname: z.string(),
  emails: z.array(z.string()),
  status: z.enum(['registered', 'active', 'inactive', 'withdrawn']),
  created_at: z.iso.datetime({ offset: true }),
})), page: z.number().int().positive(), page_size: z.number().int().positive(), has_next: z.boolean() }) })

export const userKeys = { all: ['users'] as const }

export type UserFilters = { name: string; email: string; status: string; page: number }

export function userListOptions(token: string, filters: UserFilters) {
  return queryOptions({
    queryKey: [...userKeys.all, 'list', filters],
    retry: false,
    queryFn: async ({ signal }) => responseSchema.parse(await apiClient.get('/api/auth/users', {
      headers: { Authorization: `Bearer ${token}` }, signal,
      searchParams: { name: filters.name, email: filters.email, ...(filters.status ? { status: filters.status } : {}), page: filters.page },
    }).json()).data,
  })
}
