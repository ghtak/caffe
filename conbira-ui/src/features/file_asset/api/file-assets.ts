import { mutationOptions, queryOptions } from '@tanstack/react-query'
import { isHTTPError } from 'ky'
import { z } from 'zod'
import { apiClient } from '@/shared/api/client'

export const fileAssetKeys = { all: ['file-assets'] as const }
const assetSchema = z.object({
  id: z.uuid(),
  status: z.enum(['pending', 'attached', 'deleted']),
  uploaded_by: z.number(),
  original_filename: z.string(),
  size_bytes: z.number(),
  content_type: z.string(),
  created_at: z.iso.datetime({ offset: true }),
  updated_at: z.iso.datetime({ offset: true }),
})
const ticketSchema = z.object({
  headers: z.array(z.tuple([z.string(), z.string()])),
  expires_in_seconds: z.number(),
})
const auth = (token: string) => ({ Authorization: `Bearer ${token}` })

export function fileAssetListOptions(token: string, page: number) {
  return queryOptions({
    meta: { requiresSession: true },
    queryKey: [...fileAssetKeys.all, 'list', page],
    retry: false,
    queryFn: async ({ signal }) =>
      z
        .object({
          code: z.literal('OK'),
          data: z.object({
            items: z.array(assetSchema),
            page: z.number(),
            page_size: z.number(),
            has_next: z.boolean(),
          }),
        })
        .parse(
          await apiClient
            .get('/api/file-assets', { headers: auth(token), searchParams: { page }, signal })
            .json(),
        ).data,
  })
}

export function uploadFileAssetOptions(token: string) {
  return mutationOptions({
    mutationFn: async (file: File) => {
      const contentType = file.type || 'application/octet-stream'
      const ticket = z
        .object({
          code: z.literal('OK'),
          data: ticketSchema.extend({ id: z.uuid(), upload_url: z.url({ protocol: /^https?$/ }) }),
        })
        .parse(
          await apiClient
            .post('/api/file-assets', {
              headers: auth(token),
              json: {
                original_filename: file.name,
                size_bytes: file.size,
                content_type: contentType,
              },
              retry: 0,
            })
            .json(),
        ).data
      const response = await fetch(ticket.upload_url, {
        method: 'PUT',
        headers: ticket.headers,
        body: file,
        credentials: 'omit',
      })
      if (!response.ok) throw new Error('스토리지 업로드에 실패했습니다.')
    },
  })
}

export function deleteFileAssetOptions(token: string) {
  return mutationOptions({
    mutationFn: async (id: string) => {
      await apiClient.delete(`/api/file-assets/${id}`, { headers: auth(token), retry: 0 })
    },
  })
}

export function cleanupFileAssetsOptions(token: string) {
  return mutationOptions({
    mutationFn: async () => {
      await apiClient.post('/api/file-assets/cleanup', { headers: auth(token), retry: 0 })
    },
  })
}

export function attachFileAssetOptions(token: string) {
  return mutationOptions({
    mutationFn: async (id: string) => {
      await apiClient.post(`/api/file-assets/${id}/attach`, { headers: auth(token), retry: 0 })
    },
  })
}

export function downloadFileAssetOptions(token: string) {
  return mutationOptions({
    mutationFn: async ({ id, filename }: { id: string; filename: string }) => {
      const ticket = z
        .object({
          code: z.literal('OK'),
          data: ticketSchema.extend({ download_url: z.url({ protocol: /^https?$/ }) }),
        })
        .parse(
          await apiClient
            .get(`/api/file-assets/${id}/download`, { headers: auth(token), retry: 0 })
            .json(),
        ).data
      const link = document.createElement('a')
      link.href = ticket.download_url
      link.download = filename
      link.click()
    },
  })
}

export function fileAssetError(error: unknown) {
  if (isHTTPError(error)) {
    if (error.response.status === 403) return '이 작업을 수행할 권한이 없습니다.'
    if (error.response.status === 404) return '파일을 찾을 수 없습니다. 목록을 새로고침해 주세요.'
    if (error.response.status === 400)
      return '요청을 처리할 수 없습니다. 파일 상태와 업로드 조건을 확인해 주세요.'
  }
  return '요청을 처리하지 못했습니다. 다시 시도해 주세요.'
}
