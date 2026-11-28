import { zodResolver } from '@hookform/resolvers/zod'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { isHTTPError } from 'ky'
import { useForm } from 'react-hook-form'
import { useState } from 'react'
import { z } from 'zod'
import { Alert, AlertDescription } from '@/shared/components/ui/Alert'
import { Button } from '@/shared/components/ui/Button'
import { Input } from '@/shared/components/ui/Input'
import {
  postDetailOptions,
  postKeys,
  postUpdateOptions,
  prepareAttachments,
  type Attachment,
} from '../api/posts'
import { Attachments } from '../components/Attachments'

const schema = z.object({
  title: z
    .string()
    .trim()
    .min(1, '제목을 입력해 주세요.')
    .refine((value) => [...value].length <= 255, '제목은 255자 이하로 입력해 주세요.'),
  content: z.string().refine((value) => value.trim().length > 0, '본문을 입력해 주세요.'),
})

export function PostEditForm({
  post,
  token,
  onClose,
}: {
  post: {
    id: number
    title: string
    content: string
    board_slug: string
    attachment_group_id: string | null
    attachments: Attachment[]
  }
  token: string
  onClose: () => void
}) {
  const [files, setFiles] = useState<File[]>([])
  const [removed, setRemoved] = useState<string[]>([])
  const queryClient = useQueryClient()
  const form = useForm<z.infer<typeof schema>>({
    resolver: zodResolver(schema),
    defaultValues: { title: post.title, content: post.content },
  })
  const update = useMutation({
    mutationFn: async (values: { title: string; content: string }, context) => {
      const attachments = await prepareAttachments(token, files, post.attachment_group_id, removed)
      return postUpdateOptions(post.id, token).mutationFn!({ ...values, attachments }, context)
    },
    onSuccess: (_data, values) => {
      queryClient.setQueryData(
        postDetailOptions(post.id).queryKey,
        (previous) => previous && { ...previous, ...values },
      )
      if (post.board_slug === 'data') {
        void queryClient.invalidateQueries({ queryKey: postDetailOptions(post.id).queryKey })
      }
      void queryClient.invalidateQueries({ queryKey: [...postKeys.all, 'list'] })
      onClose()
    },
  })
  return (
    <form
      className="grid gap-4 rounded-lg border bg-card p-6"
      onSubmit={form.handleSubmit((values) => update.mutate(values))}
    >
      <label className="grid gap-2 text-sm font-medium">
        제목
        <Input
          {...form.register('title')}
          disabled={update.isPending}
          aria-invalid={!!form.formState.errors.title}
        />
      </label>
      {form.formState.errors.title && (
        <p role="alert" className="text-xs text-destructive">
          {form.formState.errors.title.message}
        </p>
      )}
      <label className="grid gap-2 text-sm font-medium">
        본문
        <textarea
          {...form.register('content')}
          rows={14}
          disabled={update.isPending}
          aria-invalid={!!form.formState.errors.content}
          className="w-full resize-y rounded-md border border-input bg-background px-3 py-2 text-sm shadow-xs outline-none focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50 disabled:opacity-50 aria-invalid:border-destructive"
        />
      </label>
      {form.formState.errors.content && (
        <p role="alert" className="text-xs text-destructive">
          {form.formState.errors.content.message}
        </p>
      )}
      {post.board_slug === 'data' && (
        <>
          {post.attachments
            .filter((file) => !removed.includes(file.file_id))
            .map((file) => (
              <div key={file.file_id} className="flex items-center justify-between gap-2 text-sm">
                <span>{file.original_filename}</span>
                <Button
                  type="button"
                  variant="ghost"
                  disabled={update.isPending}
                  aria-label={`${file.original_filename} 삭제`}
                  onClick={() => setRemoved([...removed, file.file_id])}
                >
                  삭제
                </Button>
              </div>
            ))}
          <Attachments files={files} onChange={setFiles} disabled={update.isPending} />
        </>
      )}
      {update.isError && (
        <Alert variant="destructive">
          <AlertDescription>
            {isHTTPError(update.error) && update.error.response.status === 403
              ? '글을 수정할 권한이 없습니다.'
              : isHTTPError(update.error) && update.error.response.status === 401
                ? '로그인이 필요합니다. 다시 로그인해 주세요.'
                : '글을 수정하지 못했습니다. 다시 시도해 주세요.'}
          </AlertDescription>
        </Alert>
      )}
      <div className="flex justify-end gap-2">
        <Button type="button" variant="outline" disabled={update.isPending} onClick={onClose}>
          취소
        </Button>
        <Button type="submit" disabled={update.isPending}>
          {update.isPending ? '저장 중…' : '저장'}
        </Button>
      </div>
    </form>
  )
}
