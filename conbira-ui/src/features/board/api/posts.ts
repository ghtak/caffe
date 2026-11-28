import { mutationOptions, queryOptions } from '@tanstack/react-query'
import { z } from 'zod'
import { apiClient } from '@/shared/api/client'

const attachmentSchema = z.object({
  file_id: z.uuid(),
  group_id: z.uuid(),
  original_filename: z.string(),
})
export type Attachment = z.infer<typeof attachmentSchema>
type AttachmentChanges = { group_id: string; attach_file_ids: string[]; detach_file_ids: string[] }
export async function prepareAttachments(
  token: string,
  files: File[],
  groupId: string | null,
  removed: string[],
): Promise<AttachmentChanges | undefined> {
  if (!files.length && !removed.length) return undefined
  const body = new FormData()
  for (const file of files) body.append('file', file, file.name)
  if (groupId) body.append('group_id', groupId)
  const uploaded = files.length
    ? z
        .object({ data: z.array(attachmentSchema).min(1) })
        .parse(
          await apiClient
            .post('/api/files', { headers: { Authorization: `Bearer ${token}` }, body, retry: 0 })
            .json(),
        ).data
    : []
  groupId = groupId ?? uploaded[0]?.group_id ?? null
  if (!groupId) throw new Error('Missing file group')
  return {
    group_id: groupId,
    attach_file_ids: uploaded.map((f) => f.file_id),
    detach_file_ids: removed,
  }
}
export async function downloadAttachment(token: string, id: string) {
  const response = z.object({ data: z.object({ url: z.url({ protocol: /^https?$/ }) }) }).parse(
    await apiClient
      .get(`/api/files/${id}/download`, {
        headers: { Authorization: `Bearer ${token}` },
        retry: 0,
      })
      .json(),
  )
  window.location.assign(response.data.url)
}
const postSchema = z.object({
  id: z.number().int(),
  title: z.string(),
  author_id: z.number().int(),
  board_slug: z.string(),
  attachment_group_id: z.uuid().nullable().default(null),
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
