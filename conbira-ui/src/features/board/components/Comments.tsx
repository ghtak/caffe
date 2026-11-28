import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import { isHTTPError } from 'ky'
import { useState, type FormEvent } from 'react'
import { Alert, AlertDescription } from '@/shared/components/ui/Alert'
import { Button } from '@/shared/components/ui/Button'
import {
  commentCreateOptions,
  commentKey,
  commentListOptions,
  commentUpdateOptions,
} from '../api/comments'
import { postKeys } from '../api/posts'

function CommentForm({
  postId,
  token,
  parentId,
  commentId,
  initialContent = '',
  onDone,
}: {
  postId: number
  token?: string
  parentId?: number
  commentId?: number
  initialContent?: string
  onDone?: () => void
}) {
  const [content, setContent] = useState(initialContent)
  const [invalid, setInvalid] = useState(false)
  const queryClient = useQueryClient()
  const create = useMutation({
    ...commentCreateOptions(postId, token ?? ''),
    onSuccess: () => {
      setContent('')
      void queryClient.invalidateQueries({ queryKey: commentKey(postId) })
      void queryClient.invalidateQueries({ queryKey: postKeys.all })
      onDone?.()
    },
  })
  const update = useMutation({
    ...commentUpdateOptions(commentId ?? 0, token ?? ''),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: commentKey(postId) })
      onDone?.()
    },
  })
  const pending = create.isPending || update.isPending
  const error = create.error ?? update.error
  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (!token || !content.trim()) {
      setInvalid(true)
      return
    }
    setInvalid(false)
    if (commentId !== undefined) update.mutate({ content })
    else create.mutate({ content, ...(parentId === undefined ? {} : { parent_id: parentId }) })
  }
  return (
    <form onSubmit={submit} className="grid gap-2">
      <label className="grid gap-2 text-sm font-medium">
        {commentId !== undefined ? '댓글 수정' : parentId === undefined ? '댓글' : '답글'}
        <textarea
          value={content}
          onChange={(event) => setContent(event.target.value)}
          disabled={!token || pending}
          aria-invalid={invalid}
          rows={3}
          className="w-full resize-y rounded-md border border-input bg-background px-3 py-2 text-sm focus-visible:ring-[3px] focus-visible:ring-ring/50 disabled:opacity-50"
        />
      </label>
      {!token && (
        <p className="text-sm text-muted-foreground">
          댓글을 작성하려면{' '}
          <Link className="text-primary underline" to="/login">
            로그인
          </Link>
          해 주세요.
        </p>
      )}
      {invalid && (
        <p role="alert" className="text-sm text-destructive">
          내용을 입력해 주세요.
        </p>
      )}
      {error && (
        <Alert variant="destructive">
          <AlertDescription>
            {isHTTPError(error) && error.response.status === 401
              ? '로그인이 필요합니다. 다시 로그인해 주세요.'
              : `댓글을 ${commentId === undefined ? '등록' : '수정'}하지 못했습니다. 다시 시도해 주세요.`}
          </AlertDescription>
        </Alert>
      )}
      <div className="flex justify-end gap-2">
        {onDone && (
          <Button type="button" variant="outline" disabled={pending} onClick={onDone}>
            취소
          </Button>
        )}
        <Button type="submit" disabled={!token || pending}>
          {commentId !== undefined
            ? pending
              ? '저장 중…'
              : '저장'
            : pending
              ? '등록 중…'
              : '등록'}
        </Button>
      </div>
    </form>
  )
}

export function Comments({
  postId,
  token,
  userId,
}: {
  postId: number
  token?: string
  userId?: number
}) {
  const list = useQuery(commentListOptions(postId))
  const [editing, setEditing] = useState<number | null>(null)
  const [reply, setReply] = useState<{ target: number; parent: number; initial: string } | null>(
    null,
  )
  return (
    <section aria-label="댓글 목록" className="grid gap-4">
      <h2 className="text-lg font-semibold">댓글</h2>
      {list.isPending ? (
        <p role="status" className="text-sm text-muted-foreground">
          댓글을 불러오는 중…
        </p>
      ) : list.isError ? (
        <div>
          <Alert variant="destructive">
            <AlertDescription>댓글을 불러오지 못했습니다.</AlertDescription>
          </Alert>
          <Button variant="outline" onClick={() => void list.refetch()}>
            다시 시도
          </Button>
        </div>
      ) : list.data.length === 0 ? (
        <p className="text-sm text-muted-foreground">등록된 댓글이 없습니다.</p>
      ) : (
        <ul className="grid gap-3">
          {list.data.map((comment) => (
            <li
              key={comment.id}
              className={
                comment.parent_id === null
                  ? 'rounded-lg border bg-card px-4 py-2'
                  : 'ml-6 rounded-lg border bg-card px-4 py-2'
              }
            >
              <div className="flex items-start justify-between gap-2">
                <div className="text-sm">
                  <span className="font-semibold">{comment.author_nickname}</span>{' '}
                  <time className="text-muted-foreground" dateTime={comment.created_at}>
                    {new Date(comment.created_at).toLocaleString('ko-KR')}
                  </time>
                  {new Date(comment.updated_at).getTime() >
                    new Date(comment.created_at).getTime() && (
                    <span className="ml-1 text-muted-foreground">(수정됨)</span>
                  )}
                </div>
                <div className="flex shrink-0">
                  {token && userId === comment.author_id && (
                    <Button
                      type="button"
                      variant="ghost"
                      size="sm"
                      onClick={() => {
                        setEditing(comment.id)
                        setReply(null)
                      }}
                    >
                      수정
                    </Button>
                  )}
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    disabled={!token}
                    onClick={() => {
                      setEditing(null)
                      setReply({
                        target: comment.id,
                        parent: comment.parent_id ?? comment.id,
                        initial: comment.parent_id === null ? '' : `@${comment.author_nickname} `,
                      })
                    }}
                  >
                    답글달기
                  </Button>
                </div>
              </div>
              {editing === comment.id ? (
                <div className="mt-2">
                  <CommentForm
                    postId={postId}
                    token={token}
                    commentId={comment.id}
                    initialContent={comment.content}
                    onDone={() => setEditing(null)}
                  />
                </div>
              ) : (
                <p className="text-sm break-words whitespace-pre-wrap">{comment.content}</p>
              )}
              {reply?.target === comment.id && (
                <div className="mt-4">
                  <CommentForm
                    key={comment.id}
                    postId={postId}
                    token={token}
                    parentId={reply.parent}
                    initialContent={reply.initial}
                    onDone={() => setReply(null)}
                  />
                </div>
              )}
            </li>
          ))}
        </ul>
      )}
      <CommentForm postId={postId} token={token} />
    </section>
  )
}
