import { useQuery } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import { isHTTPError } from 'ky'
import { useState } from 'react'
import { Alert, AlertDescription } from '@/shared/components/ui/Alert'
import { Button } from '@/shared/components/ui/Button'
import { postDetailOptions } from '../api/posts'
import { PostEditForm } from './PostEditForm'
import { Comments } from '../components/Comments'

export function PostDetailPage({
  id,
  token,
  userId,
}: {
  id: number
  token?: string
  userId?: number
}) {
  const detail = useQuery(postDetailOptions(id))
  const [editing, setEditing] = useState(false)
  if (detail.isPending)
    return (
      <div className="mx-auto flex w-full max-w-6xl flex-col px-4 py-6 sm:px-6 sm:py-8 lg:px-8">
        <div role="status" className="rounded-lg border bg-card p-6 text-sm text-muted-foreground">
          게시글을 불러오는 중…
        </div>
      </div>
    )
  if (detail.isError)
    return (
      <section className="mx-auto flex w-full max-w-6xl flex-col items-start gap-4 px-4 py-6 sm:px-6 sm:py-8 lg:px-8">
        <Alert variant="destructive">
          <AlertDescription>
            {isHTTPError(detail.error) && detail.error.response.status === 404
              ? '게시글을 찾을 수 없습니다.'
              : '게시글을 불러오지 못했습니다.'}
          </AlertDescription>
        </Alert>
        <div className="flex gap-2">
          <Button variant="outline" onClick={() => void detail.refetch()}>
            다시 시도
          </Button>
          <Button asChild variant="outline">
            <Link to="/community">목록으로</Link>
          </Button>
        </div>
      </section>
    )
  const post = detail.data
  const board =
    post.board_slug === 'notice'
      ? '공지'
      : post.board_slug === 'free'
        ? '자유게시판'
        : post.board_slug === 'data'
          ? '자료실'
          : post.board_slug
  const listPath =
    post.board_slug === 'notice'
      ? '/community/notice'
      : post.board_slug === 'free'
        ? '/community/free'
        : post.board_slug === 'data'
          ? '/community/data'
          : '/community'
  return (
    <article className="mx-auto flex w-full max-w-6xl min-w-0 flex-col gap-6 px-4 py-6 sm:px-6 sm:py-8 lg:px-8">
      <header className="grid gap-3 border-b pb-5">
        <p className="text-sm font-medium text-primary">커뮤니티 / {board}</p>
        <h1 className="text-2xl font-bold tracking-tight break-words sm:text-3xl">{post.title}</h1>
        <dl className="flex flex-wrap gap-x-6 gap-y-2 text-sm text-muted-foreground">
          <div className="flex gap-2">
            <dt>작성자</dt>
            <dd>{post.author_nickname}</dd>
          </div>
          <div className="flex gap-2">
            <dt>작성일</dt>
            <dd>{new Date(post.created_at).toLocaleString('ko-KR')}</dd>
          </div>
          <div className="flex gap-2">
            <dt>조회수</dt>
            <dd>{post.view_count}</dd>
          </div>
        </dl>
      </header>
      {editing && token && userId === post.author_id ? (
        <PostEditForm post={post} token={token} onClose={() => setEditing(false)} />
      ) : (
        <section
          aria-label="본문"
          className="rounded-lg border bg-card p-6 text-sm leading-6 break-words whitespace-pre-wrap"
        >
          {post.content}
        </section>
      )}
      {post.board_slug === 'free' && (
        <Comments key={id} postId={id} token={token} userId={userId} />
      )}
      <footer>
        <Button asChild variant="outline">
          <Link to={listPath}>목록으로</Link>
        </Button>
        {!editing && token && userId === post.author_id && (
          <Button className="ml-2" onClick={() => setEditing(true)}>
            수정
          </Button>
        )}
      </footer>
    </article>
  )
}
