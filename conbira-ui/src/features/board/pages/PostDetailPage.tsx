import { useQuery } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import { isHTTPError } from 'ky'
import { Button } from '@/shared/components/ui/Button'
import { postDetailOptions } from '../api/posts'

export function PostDetailPage({ id }: { id: number }) {
  const detail = useQuery(postDetailOptions(id))
  if (detail.isPending)
    return (
      <p role="status" className="p-8">
        게시글을 불러오는 중…
      </p>
    )
  if (detail.isError)
    return (
      <section className="grid justify-items-start gap-4 p-8">
        <p role="alert">
          {isHTTPError(detail.error) && detail.error.response.status === 404
            ? '게시글을 찾을 수 없습니다.'
            : '게시글을 불러오지 못했습니다.'}
        </p>
        <Button variant="outline" onClick={() => void detail.refetch()}>
          다시 시도
        </Button>
        <Button asChild variant="outline">
          <Link to="/community">목록으로</Link>
        </Button>
      </section>
    )
  const post = detail.data
  const board =
    post.board_slug === 'notice'
      ? '공지'
      : post.board_slug === 'free'
        ? '자유게시판'
        : post.board_slug
  const listPath =
    post.board_slug === 'notice'
      ? '/community/notice'
      : post.board_slug === 'free'
        ? '/community/free'
        : '/community'
  return (
    <article className="mx-auto grid w-full max-w-7xl min-w-0 content-start gap-8 p-5 sm:p-8 lg:p-12">
      <header className="grid gap-3 border-b pb-5">
        <p className="text-sm font-medium text-primary">커뮤니티 / {board}</p>
        <h1 className="break-words text-3xl font-bold tracking-tight">{post.title}</h1>
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
      <section aria-label="본문" className="whitespace-pre-wrap break-words leading-relaxed">
        {post.content}
      </section>
      <footer>
        <Button asChild variant="outline">
          <Link to={listPath}>목록으로</Link>
        </Button>
      </footer>
    </article>
  )
}
