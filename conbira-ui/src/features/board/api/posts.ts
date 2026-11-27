import { queryOptions } from '@tanstack/react-query'
import { z } from 'zod'
import { apiClient } from '@/shared/api/client'

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
    items: z.array(postSchema),
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
export function postListOptions(boardSlug: 'notice' | 'free', filters: PostFilters) {
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
