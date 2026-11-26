import { mutationOptions, queryOptions } from '@tanstack/react-query'
import { isHTTPError } from 'ky'
import { z } from 'zod'
import { apiClient } from '@/shared/api/client'
import { fileSummarySchema, type AttachFilesRequest } from '../types/files.types'

export const fileKeys = { all: ['files'] as const }
const uploadResponseSchema = z.object({ code: z.literal('OK'), data: z.array(fileSummarySchema) })

const listSchema = z.object({
  code: z.literal('OK'),
  data: z.object({
    items: z.array(fileSummarySchema),
    page: z.number().int().positive(),
    page_size: z.number().int().positive(),
    has_next: z.boolean(),
  }),
})
export type FileFilters = { name: string; page: number }
export function fileListOptions(token: string, filters: FileFilters) {
  return queryOptions({
    queryKey: [...fileKeys.all, 'list', filters],
    retry: false,
    queryFn: async ({ signal }) =>
      listSchema.parse(
        await apiClient
          .get('/api/files', {
            headers: { Authorization: `Bearer ${token}` },
            signal,
            searchParams: filters,
          })
          .json(),
      ).data,
  })
}

type FileChange =
  | { action: 'upload'; files: FileList; groupId: string }
  | { action: 'attach'; groupId: string; body: AttachFilesRequest }
  | { action: 'delete'; fileId: string }

export function fileChangeOptions(token: string) {
  return mutationOptions({
    mutationFn: async (change: FileChange) => {
      const headers = { Authorization: `Bearer ${token}` }
      switch (change.action) {
        case 'upload': {
          const body = new FormData()
          for (const file of Array.from(change.files)) body.append('file', file, file.name)
          if (change.groupId) body.append('group_id', change.groupId)
          uploadResponseSchema.parse(
            await apiClient.post('/api/files', { headers, body, retry: 0 }).json(),
          )
          break
        }
        case 'attach':
          await apiClient.post(`/api/file-groups/${change.groupId}/attach`, {
            headers,
            json: change.body,
            retry: 0,
          })
          break
        case 'delete':
          await apiClient.delete(`/api/files/${change.fileId}`, { headers, retry: 0 })
          break
      }
    },
  })
}

export function fileDownloadOptions(token: string) {
  return mutationOptions({
    mutationFn: async (fileId: string) =>
      z
        .object({
          code: z.literal('OK'),
          data: z.object({
            url: z.url({ protocol: /^https?$/ }),
            expires_at: z.iso.datetime({ offset: true }),
          }),
        })
        .parse(
          await apiClient
            .get(`/api/files/${fileId}/download`, {
              headers: { Authorization: `Bearer ${token}` },
              retry: 0,
            })
            .json(),
        ).data,
  })
}

export function fileErrorMessage(error: unknown) {
  if (isHTTPError(error)) {
    switch (error.response.status) {
      case 401:
        return '로그인 세션이 만료되었습니다.'
      case 403:
        return '이 작업을 수행할 권한이 없습니다.'
      case 404:
        return '파일 또는 그룹을 찾을 수 없습니다. 목록을 새로고침해 주세요.'
      case 413:
        return '업로드 가능한 파일 크기를 초과했습니다.'
      case 400:
        return '요청을 처리할 수 없습니다. 파일 상태, 그룹과 연결 대상을 확인해 주세요.'
    }
  }
  return '요청을 처리하지 못했습니다. 다시 시도해 주세요.'
}
