import { queryOptions } from '@tanstack/react-query'
import { apiClient } from '@/shared/api/client'
import { catalogResponse, type Catalog } from '../types/catalog.types'

export const rbacKeys = {
  all: ['rbac'] as const,
  catalog: (catalog: Catalog) => [...rbacKeys.all, catalog] as const,
}

export function catalogOptions(catalog: Catalog, token: string) {
  return queryOptions({
    meta: { requiresSession: true },
    queryKey: rbacKeys.catalog(catalog),
    retry: false,
    queryFn: async ({ signal }) => {
      const body = await apiClient
        .get(`/api/rbac/${catalog}`, {
          headers: { Authorization: `Bearer ${token}` },
          signal,
        })
        .json()
      return catalogResponse.parse(body).data
    },
  })
}
