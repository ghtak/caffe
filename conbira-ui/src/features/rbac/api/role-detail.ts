import { mutationOptions, queryOptions } from '@tanstack/react-query'
import { z } from 'zod'
import { apiClient } from '@/shared/api/client'
import { catalogOptions } from './catalog'
import { catalogItemSchema } from '../types/catalog.types'

const roleUserSchema = z.object({
  id: z.number().int(),
  nickname: z.string(),
  emails: z.array(z.string()),
})
const candidatePageSchema = z.object({
  items: z.array(roleUserSchema.extend({ assigned: z.boolean() })),
  has_next: z.boolean(),
})
const roleUserPageSchema = z.object({ items: z.array(roleUserSchema), has_next: z.boolean() })
const headers = (token: string) => ({ Authorization: `Bearer ${token}` })
const get = async <T>(path: string, token: string, schema: z.ZodType<T>, signal: AbortSignal) =>
  schema.parse(await apiClient.get(path, { headers: headers(token), signal }).json())
export const roleKeys = { detail: (id: number) => ['rbac', 'role', id] as const }
export const roleDetailOptions = (id: number, token: string) =>
  queryOptions({
    meta: { requiresSession: true },
    queryKey: [...roleKeys.detail(id), 'detail'],
    retry: false,
    queryFn: async ({ signal }) =>
      (await get(`/api/rbac/roles/${id}`, token, z.object({ data: catalogItemSchema }), signal))
        .data,
  })
export const rolePermissionsOptions = (id: number, token: string) =>
  queryOptions({
    meta: { requiresSession: true },
    queryKey: [...roleKeys.detail(id), 'permissions'],
    retry: false,
    queryFn: async ({ signal }) =>
      (
        await get(
          `/api/rbac/roles/${id}/permissions`,
          token,
          z.object({ data: z.array(catalogItemSchema) }),
          signal,
        )
      ).data,
  })
export const permissionsOptions = (token: string) => catalogOptions('permissions', token)
export const roleUsersOptions = (id: number, token: string, number: number) =>
  queryOptions({
    meta: { requiresSession: true },
    queryKey: [...roleKeys.detail(id), 'users', number],
    retry: false,
    queryFn: async ({ signal }) =>
      (
        await get(
          `/api/rbac/roles/${id}/users?page=${number}`,
          token,
          z.object({ data: roleUserPageSchema }),
          signal,
        )
      ).data,
  })
export const candidateOptions = (id: number, token: string, name: string, number: number) =>
  queryOptions({
    meta: { requiresSession: true },
    queryKey: [...roleKeys.detail(id), 'candidates', name, number],
    retry: false,
    queryFn: async ({ signal }) =>
      (
        await get(
          `/api/rbac/roles/${id}/users/search?${new URLSearchParams({ name, page: String(number) })}`,
          token,
          z.object({ data: candidatePageSchema }),
          signal,
        )
      ).data,
  })
export const rolePermissionChangeOptions = (roleId: number, token: string) =>
  mutationOptions({
    mutationFn: async ({ permissionId, assigned }: { permissionId: number; assigned: boolean }) => {
      await apiClient(`/api/rbac/roles/${roleId}/permissions/${permissionId}`, {
        method: assigned ? 'post' : 'delete',
        headers: headers(token),
        retry: 0,
      })
    },
  })
export const userRoleChangeOptions = (roleId: number, token: string) =>
  mutationOptions({
    mutationFn: async ({ userId, assigned }: { userId: number; assigned: boolean }) => {
      await apiClient(`/api/rbac/users/${userId}/roles/${roleId}`, {
        method: assigned ? 'post' : 'delete',
        headers: headers(token),
        retry: 0,
      })
    },
  })
