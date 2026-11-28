import { POST_PAGE_SIZE } from '../constants'
import { mutationOptions, queryOptions } from '@tanstack/react-query'
import { z } from 'zod'
import { apiClient } from '@/shared/api/client'

const attachmentSchema = z.object({
  id: z.uuid(),
  original_filename: z.string(),
})
export type Attachment = z.infer<typeof attachmentSchema>
const uploadTicketResponseSchema = z.object({
  data: z.object({
    id: z.uuid(),
    upload_url: z.url({ protocol: /^https?$/ }),
    headers: z.array(z.tuple([z.string(), z.string()])),
  }),
})
const downloadTicketResponseSchema = z.object({
  data: z.object({
    download_url: z.url({ protocol: /^https?$/ }),
    headers: z.array(z.tuple([z.string(), z.string()])),
  }),
})
type AttachmentChanges = { attach_file_ids: string[]; detach_file_ids: string[] }
export async function prepareAttachments(
  token: string,
  files: File[],
  boardSlug: string,
  removed: string[],
): Promise<AttachmentChanges | undefined> {
  if (!files.length && !removed.length) return undefined
  const ids: string[] = []
  for (const file of files) {
    const contentType = file.type || 'application/octet-stream'
    const ticket = uploadTicketResponseSchema.parse(
      await apiClient
        .post('/api/board/posts/attachments/upload', {
          headers: { Authorization: `Bearer ${token}` },
          json: {
            board_slug: boardSlug,
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
    if (!response.ok) throw new Error('Upload failed')
    ids.push(ticket.id)
  }
  return { attach_file_ids: ids, detach_file_ids: removed }
}
export async function downloadAttachment(
  token: string,
  postId: number,
  id: string,
  filename: string,
) {
  const ticket = downloadTicketResponseSchema.parse(
    await apiClient
      .get(`/api/board/posts/${postId}/attachments/${id}`, {
        headers: { Authorization: `Bearer ${token}` },
        retry: 0,
      })
      .json(),
  ).data
  const link = document.createElement('a')
  link.href = ticket.download_url
  link.download = filename
  link.click()
}
const postSchema = z.object({
  id: z.number().int(),
  title: z.string(),
  author_id: z.number().int(),
  board_slug: z.string(),
  view_count: z.number().int(),
  comment_count: z.number().int(),
  created_at: z.iso.datetime({ offset: true }),
})
const postResponseSchema = z.object({ code: z.literal('OK'), data: postSchema })
const postDetailResponseSchema = z.object({
  code: z.literal('OK'),
  data: postSchema.extend({
    author_nickname: z.string(),
    content: z.string(),
    attachments: z.array(attachmentSchema).default([]),
  }),
})
const listSchema = z.object({
  code: z.literal('OK'),
  data: z.object({
    items: z.array(postSchema.extend({ author_nickname: z.string() })),
    page: z.number().int().positive(),
    page_size: z.number().int().positive(),
    has_next: z.boolean(),
  }),
})
export type PostFilters = { search: string; author_id: string; page: number }
export const postKeys = {
  all: ['posts'] as const,
  lists: ['posts', 'list'] as const,
  detail: (id: number) => ['posts', 'detail', id] as const,
  list: (boardSlug: string, filters: PostFilters) => ['posts', 'list', boardSlug, filters] as const,
}
export function postListOptions(boardSlug: 'notice' | 'free' | 'data', filters: PostFilters) {
  return queryOptions({
    queryKey: postKeys.list(boardSlug, filters),
    retry: false,
    queryFn: async ({ signal }) => {
      const searchParams = new URLSearchParams({
        board_slug: boardSlug,
        search: filters.search,
        page: String(filters.page),
        page_size: String(POST_PAGE_SIZE),
      })
      if (filters.author_id) searchParams.set('author_id', filters.author_id)
      return listSchema.parse(
        await apiClient.get('/api/board/posts', { searchParams, signal, retry: 0 }).json(),
      ).data
    },
  })
}

export function postCreateOptions(token: string) {
  return mutationOptions({
    mutationFn: async (body: {
      board_slug: 'notice' | 'free' | 'data'
      title: string
      content: string
      files: File[]
    }) => {
      const { files, ...values } = body
      const attachments = await prepareAttachments(token, files, values.board_slug, [])
      return postResponseSchema.parse(
        await apiClient
          .post('/api/board/posts', {
            headers: { Authorization: `Bearer ${token}` },
            json: { ...values, attachments },
            retry: 0,
          })
          .json(),
      ).data
    },
  })
}

export function postDetailOptions(id: number) {
  return queryOptions({
    queryKey: postKeys.detail(id),
    retry: false,
    queryFn: async ({ signal }) =>
      postDetailResponseSchema.parse(
        await apiClient.get(`/api/board/posts/${id}`, { signal, retry: 0 }).json(),
      ).data,
  })
}

export function postUpdateOptions(id: number, boardSlug: string, token: string) {
  return mutationOptions({
    mutationFn: async (body: {
      title: string
      content: string
      files: File[]
      removed: string[]
    }) => {
      const { files, removed, ...values } = body
      const attachments = await prepareAttachments(token, files, boardSlug, removed)
      return postResponseSchema.parse(
        await apiClient
          .put(`/api/board/posts/${id}`, {
            headers: { Authorization: `Bearer ${token}` },
            json: { ...values, attachments },
            retry: 0,
          })
          .json(),
      ).data
    },
  })
}
