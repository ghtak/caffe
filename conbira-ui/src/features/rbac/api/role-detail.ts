import { mutationOptions, queryOptions } from '@tanstack/react-query'
import { z } from 'zod'
import { apiClient } from '@/shared/api/client'

const item = z.object({
  id: z.number().int(),
  code: z.string(),
  name: z.string(),
  description: z.string(),
})
const user = z.object({ id: z.number().int(), nickname: z.string(), emails: z.array(z.string()) })
const candidatePage = z.object({
  items: z.array(user.extend({ assigned: z.boolean() })),
  has_next: z.boolean(),
})
const page = z.object({ items: z.array(user), has_next: z.boolean() })
const headers = (token: string) => ({ Authorization: `Bearer ${token}` })
const get = async <T>(path: string, token: string, schema: z.ZodType<T>, signal: AbortSignal) =>
  schema.parse(await apiClient.get(path, { headers: headers(token), signal }).json())
export const roleKeys = { detail: (id: number) => ['rbac', 'role', id] as const }
export const roleDetailOptions = (id: number, token: string) =>
  queryOptions({
    queryKey: [...roleKeys.detail(id), 'detail'],
    retry: false,
    queryFn: async ({ signal }) =>
      (await get(`/api/rbac/roles/${id}`, token, z.object({ data: item }), signal)).data,
  })
export const rolePermissionsOptions = (id: number, token: string) =>
  queryOptions({
    queryKey: [...roleKeys.detail(id), 'permissions'],
    retry: false,
    queryFn: async ({ signal }) =>
      (
        await get(
          `/api/rbac/roles/${id}/permissions`,
          token,
          z.object({ data: z.array(item) }),
          signal,
        )
      ).data,
  })
export const permissionsOptions = (token: string) =>
  queryOptions({
    queryKey: ['rbac', 'permissions', 'with-id'],
    retry: false,
    queryFn: async ({ signal }) =>
      (await get('/api/rbac/permissions', token, z.object({ data: z.array(item) }), signal)).data,
  })
export const roleUsersOptions = (id: number, token: string, number: number) =>
  queryOptions({
    queryKey: [...roleKeys.detail(id), 'users', number],
    retry: false,
    queryFn: async ({ signal }) =>
      (
        await get(
          `/api/rbac/roles/${id}/users?page=${number}`,
          token,
          z.object({ data: page }),
          signal,
        )
      ).data,
  })
export const candidateOptions = (id: number, token: string, name: string, number: number) =>
  queryOptions({
    queryKey: [...roleKeys.detail(id), 'candidates', name, number],
    retry: false,
    queryFn: async ({ signal }) =>
      (
        await get(
          `/api/rbac/roles/${id}/users/search?${new URLSearchParams({ name, page: String(number) })}`,
          token,
          z.object({ data: candidatePage }),
          signal,
        )
      ).data,
  })
export const roleChangeOptions = (token: string) =>
  mutationOptions({
    mutationFn: async ({ path, add }: { path: string; add: boolean }) => {
      await apiClient(path, { method: add ? 'post' : 'delete', headers: headers(token) })
    },
  })
