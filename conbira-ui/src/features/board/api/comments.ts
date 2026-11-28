import { mutationOptions, queryOptions } from '@tanstack/react-query'
import { z } from 'zod'
import { apiClient } from '@/shared/api/client'

const commentSchema = z.object({
  id: z.number().int(),
  post_id: z.number().int(),
  parent_id: z.number().int().nullable(),
  author_id: z.number().int(),
  author_nickname: z.string(),
  content: z.string(),
  created_at: z.iso.datetime({ offset: true }),
})
export const commentKey = (postId: number) => ['comments', postId] as const
export function commentListOptions(postId: number) {
  return queryOptions({
    queryKey: commentKey(postId),
    retry: false,
    queryFn: async ({ signal }) =>
      z
        .object({ code: z.literal('OK'), data: z.array(commentSchema) })
        .parse(
          await apiClient.get(`/api/board/posts/${postId}/comments`, { signal, retry: 0 }).json(),
        ).data,
  })
}
export function commentCreateOptions(postId: number, token: string) {
  return mutationOptions({
    mutationFn: async (body: { content: string; parent_id?: number }) => {
      await apiClient
        .post(`/api/board/posts/${postId}/comments`, {
          headers: { Authorization: `Bearer ${token}` },
          json: body,
          retry: 0,
        })
        .json()
    },
  })
}
