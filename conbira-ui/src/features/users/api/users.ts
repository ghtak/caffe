import { mutationOptions, queryOptions } from '@tanstack/react-query'
import { z } from 'zod'
import { apiClient } from '@/shared/api/client'

export const userStatusSchema = z.enum(['registered', 'active', 'inactive', 'withdrawn'])
export type UserStatus = z.infer<typeof userStatusSchema>
export const statusLabels: Record<UserStatus, string> = {
  registered: '가입',
  active: '활성',
  inactive: '비활성',
  withdrawn: '탈퇴',
}

const userSchema = z.object({
  id: z.number().int(),
  nickname: z.string(),
  emails: z.array(z.string()),
  status: userStatusSchema,
  created_at: z.iso.datetime({ offset: true }),
})
const responseSchema = z.object({
  data: z.object({
    items: z.array(userSchema),
    page: z.number().int().positive(),
    page_size: z.number().int().positive(),
    has_next: z.boolean(),
  }),
})

export const userKeys = { all: ['users'] as const }

export type UserFilters = { name: string; email: string; status: string; page: number }

export function userStatusMutationOptions(token: string) {
  return mutationOptions({
    mutationFn: async ({ id, status }: { id: number; status: UserStatus }) => {
      await apiClient.patch(`/api/auth/users/${id}/status`, {
        headers: { Authorization: `Bearer ${token}` },
        json: { status },
      })
    },
  })
}

export function userListOptions(token: string, filters: UserFilters) {
  return queryOptions({
    meta: { requiresSession: true },
    queryKey: [...userKeys.all, 'list', filters],
    retry: false,
    queryFn: async ({ signal }) =>
      responseSchema.parse(
        await apiClient
          .get('/api/auth/users', {
            headers: { Authorization: `Bearer ${token}` },
            signal,
            searchParams: {
              name: filters.name,
              email: filters.email,
              ...(filters.status ? { status: filters.status } : {}),
              page: filters.page,
            },
          })
          .json(),
      ).data,
  })
}

const detailResponseSchema = z.object({
  code: z.literal('OK'),
  data: userSchema.extend({
    updated_at: z.iso.datetime({ offset: true }),
  }),
})

export function userDetailOptions(token: string, id: number) {
  return queryOptions({
    meta: { requiresSession: true },
    queryKey: [...userKeys.all, 'detail', id],
    retry: false,
    queryFn: async ({ signal }) =>
      detailResponseSchema.parse(
        await apiClient
          .get(`/api/auth/users/${id}`, {
            headers: { Authorization: `Bearer ${token}` },
            signal,
          })
          .json(),
      ).data,
  })
}
