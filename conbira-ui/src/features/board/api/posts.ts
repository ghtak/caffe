import { mutationOptions, queryOptions } from '@tanstack/react-query'
import { z } from 'zod'
import { apiClient } from '@/shared/api/client'

const attachmentSchema = z.object({
  id: z.uuid(),
  original_filename: z.string(),
})
export type Attachment = z.infer<typeof attachmentSchema>
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
    const ticket = z
      .object({
        data: z.object({
          id: z.uuid(),
          upload_url: z.url({ protocol: /^https?$/ }),
          headers: z.array(z.tuple([z.string(), z.string()])),
        }),
      })
      .parse(
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
  const ticket = z
    .object({
      data: z.object({
        download_url: z.url({ protocol: /^https?$/ }),
        headers: z.array(z.tuple([z.string(), z.string()])),
      }),
    })
    .parse(
      await apiClient
        .get(`/api/board/posts/${postId}/attachments/${id}`, {
          headers: { Authorization: `Bearer ${token}` },
          retry: 0,
        })
        .json(),
    ).data
  const response = await fetch(ticket.download_url, {
    headers: ticket.headers,
    credentials: 'omit',
  })
  if (!response.ok) throw new Error('Download failed')
  const url = URL.createObjectURL(await response.blob())
  try {
    const link = document.createElement('a')
    link.href = url
    link.download = filename
    link.click()
  } finally {
    URL.revokeObjectURL(url)
  }
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
        page_size: '20',
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
      attachments?: AttachmentChanges
    }) =>
      z.object({ code: z.literal('OK'), data: postSchema }).parse(
        await apiClient
          .post('/api/board/posts', {
            headers: { Authorization: `Bearer ${token}` },
            json: body,
            retry: 0,
          })
          .json(),
      ).data,
  })
}

export function postDetailOptions(id: number) {
  return queryOptions({
    queryKey: [...postKeys.all, 'detail', id],
    retry: false,
    queryFn: async ({ signal }) =>
      z
        .object({
          code: z.literal('OK'),
          data: postSchema.extend({
            author_nickname: z.string(),
            content: z.string(),
            attachments: z.array(attachmentSchema).default([]),
          }),
        })
        .parse(await apiClient.get(`/api/board/posts/${id}`, { signal, retry: 0 }).json()).data,
  })
}

export function postUpdateOptions(id: number, token: string) {
  return mutationOptions({
    mutationFn: async (body: { title: string; content: string; attachments?: AttachmentChanges }) =>
      z.object({ code: z.literal('OK'), data: postSchema }).parse(
        await apiClient
          .put(`/api/board/posts/${id}`, {
            headers: { Authorization: `Bearer ${token}` },
            json: body,
            retry: 0,
          })
          .json(),
      ).data,
  })
}
