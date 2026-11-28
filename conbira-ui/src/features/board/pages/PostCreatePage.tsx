import { zodResolver } from '@hookform/resolvers/zod'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { Link, useNavigate } from '@tanstack/react-router'
import { isHTTPError } from 'ky'
import { useForm } from 'react-hook-form'
import { z } from 'zod'
import { Alert, AlertDescription } from '@/shared/components/ui/Alert'
import { Button } from '@/shared/components/ui/Button'
import { Input } from '@/shared/components/ui/Input'
import { PageHeader } from '@/shared/components/ui/PageHeader'
import { postCreateOptions, postKeys } from '../api/posts'

const schema = z.object({
  title: z
    .string()
    .trim()
    .min(1, '제목을 입력해 주세요.')
    .refine((value) => [...value].length <= 255, '제목은 255자 이하로 입력해 주세요.'),
  content: z.string().refine((value) => value.trim().length > 0, '본문을 입력해 주세요.'),
})
export function PostCreatePage({
  boardSlug,
  title,
  token,
}: {
  boardSlug: 'notice' | 'free' | 'data'
  title: string
  token: string
}) {
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const form = useForm<z.infer<typeof schema>>({
    resolver: zodResolver(schema),
    defaultValues: { title: '', content: '' },
  })
  const create = useMutation({
    ...postCreateOptions(token),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: postKeys.all })
      await navigate({ to: `/community/${boardSlug}` })
    },
  })
  return (
    <section className="mx-auto flex w-full max-w-6xl min-w-0 flex-col gap-6 px-4 py-6 sm:px-6 sm:py-8 lg:px-8">
      <PageHeader eyebrow={`커뮤니티 / ${title}`} title="글쓰기" />
      <form
        className="grid gap-4 rounded-lg border bg-card p-6"
        onSubmit={form.handleSubmit((values) =>
          create.mutate({ ...values, board_slug: boardSlug }),
        )}
      >
        <div className="grid gap-2">
          <label className="grid gap-2 text-sm font-medium">
            제목
            <Input
              {...form.register('title')}
              disabled={create.isPending}
              aria-invalid={!!form.formState.errors.title}
            />
          </label>
          {form.formState.errors.title && (
            <p role="alert" className="text-xs text-destructive">
              {form.formState.errors.title.message}
            </p>
          )}
        </div>
        <div className="grid gap-2">
          <label className="grid gap-2 text-sm font-medium">
            본문
            <textarea
              {...form.register('content')}
              rows={14}
              disabled={create.isPending}
              aria-invalid={!!form.formState.errors.content}
              className="w-full resize-y rounded-md border border-input bg-background px-3 py-2 text-sm shadow-xs outline-none focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50 disabled:opacity-50 aria-invalid:border-destructive"
            />
          </label>
          {form.formState.errors.content && (
            <p role="alert" className="text-xs text-destructive">
              {form.formState.errors.content.message}
            </p>
          )}
        </div>
        {create.isError && (
          <Alert variant="destructive">
            <AlertDescription>
              {isHTTPError(create.error) && create.error.response.status === 403
                ? '글을 작성할 권한이 없습니다.'
                : isHTTPError(create.error) && create.error.response.status === 401
                  ? '로그인이 필요합니다. 다시 로그인해 주세요.'
                  : '글을 등록하지 못했습니다. 다시 시도해 주세요.'}
            </AlertDescription>
          </Alert>
        )}
        <div className="flex justify-end gap-2">
          {create.isPending ? (
            <Button variant="outline" disabled>
              취소
            </Button>
          ) : (
            <Button asChild variant="outline">
              <Link to={`/community/${boardSlug}`}>취소</Link>
            </Button>
          )}
          <Button type="submit" disabled={create.isPending}>
            {create.isPending ? '등록 중…' : '등록'}
          </Button>
        </div>
      </form>
    </section>
  )
}
