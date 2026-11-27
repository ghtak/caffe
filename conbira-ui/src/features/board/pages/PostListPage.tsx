import { Link } from '@tanstack/react-router'
import { useQuery } from '@tanstack/react-query'
import { useState } from 'react'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { z } from 'zod'
import { Button } from '@/shared/components/ui/Button'
import { Input } from '@/shared/components/ui/Input'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/shared/components/ui/Table'
import { postListOptions } from '../api/posts'

const searchSchema = z.object({
  search: z.string().trim(),
  author_id: z
    .string()
    .trim()
    .regex(/^$|^[1-9]\d*$/, '작성자 ID는 양의 정수로 입력해 주세요.'),
})
export function PostListPage({
  boardSlug,
  title,
  canWrite = false,
}: {
  boardSlug: 'notice' | 'free'
  title: string
  canWrite?: boolean
}) {
  const [filters, setFilters] = useState({ search: '', author_id: '', page: 1 })
  const form = useForm<z.infer<typeof searchSchema>>({
    resolver: zodResolver(searchSchema),
    defaultValues: { search: '', author_id: '' },
  })
  const list = useQuery(postListOptions(boardSlug, filters))
  return (
    <section className="mx-auto grid w-full max-w-7xl min-w-0 content-start gap-8 p-5 sm:p-8 lg:p-12">
      <header className="grid gap-3">
        <p className="text-sm font-medium text-primary">커뮤니티 / {title}</p>
        <h1 className="text-3xl font-bold tracking-tight">{title}</h1>
      </header>
      <form
        aria-label="게시글 검색"
        className="flex flex-wrap items-end gap-3"
        onSubmit={form.handleSubmit((values) => setFilters({ ...values, page: 1 }))}
      >
        <label className="grid flex-1 gap-2 text-sm">
          제목·본문
          <Input {...form.register('search')} placeholder="제목 또는 본문 검색" />
        </label>
        <label className="grid gap-2 text-sm">
          작성자 ID
          <Input {...form.register('author_id')} inputMode="numeric" placeholder="전체 작성자" />
        </label>
        <Button type="submit">검색</Button>
      </form>
      {form.formState.errors.author_id && (
        <p role="alert">{form.formState.errors.author_id.message}</p>
      )}
      <section aria-label="게시글 목록" className="grid min-w-0 gap-4">
        <div className="flex items-center justify-between gap-4">
          <h2 className="font-semibold">게시글 목록</h2>
          <div className="flex gap-2">
            {canWrite && (
              <Button asChild>
                <Link to={`/community/${boardSlug}/new`}>글쓰기</Link>
              </Button>
            )}
            <Button
              variant="outline"
              disabled={list.isFetching}
              onClick={() => void list.refetch()}
            >
              새로고침
            </Button>
          </div>
        </div>
        {list.isPending ? (
          <p role="status">목록을 불러오는 중…</p>
        ) : list.isError ? (
          <div className="grid justify-items-start gap-3">
            <p role="alert">목록을 불러오지 못했습니다.</p>
            <Button variant="outline" onClick={() => void list.refetch()}>
              다시 시도
            </Button>
          </div>
        ) : list.data.items.length === 0 ? (
          <p className="rounded-lg border border-dashed p-5 text-sm text-muted-foreground">
            {filters.search || filters.author_id
              ? '검색 조건에 맞는 게시글이 없습니다.'
              : '등록된 게시글이 없습니다.'}
          </p>
        ) : (
          <Table className="min-w-2xl">
            <TableHeader>
              <TableRow>
                <TableHead scope="col">번호</TableHead>
                <TableHead scope="col" className="w-full min-w-72">
                  제목
                </TableHead>
                <TableHead scope="col">작성자</TableHead>
                <TableHead scope="col">조회수</TableHead>
                <TableHead scope="col">작성일</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {list.data.items.map((post) => (
                <TableRow key={post.id} data-testid={`board-post-${post.id}`}>
                  <TableCell>{post.id}</TableCell>
                  <TableCell className="whitespace-normal break-all">
                    <Link
                      to="/community/posts/$postId"
                      params={{ postId: String(post.id) }}
                      className="hover:underline focus-visible:underline"
                    >
                      {post.title}
                    </Link>
                    <span className="ml-1 text-muted-foreground">[{post.comment_count}]</span>
                  </TableCell>
                  <TableCell>{post.author_nickname}</TableCell>
                  <TableCell>{post.view_count}</TableCell>
                  <TableCell>{new Date(post.created_at).toLocaleString('ko-KR')}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </section>
      <nav aria-label="게시글 목록 페이지" className="flex items-center justify-center gap-3">
        <Button
          variant="outline"
          disabled={filters.page === 1 || list.isFetching}
          onClick={() => setFilters({ ...filters, page: filters.page - 1 })}
        >
          이전
        </Button>
        <span className="text-sm" aria-live="polite">
          {filters.page} 페이지
        </span>
        <Button
          variant="outline"
          disabled={!list.data?.has_next || list.isFetching}
          onClick={() => setFilters({ ...filters, page: filters.page + 1 })}
        >
          다음
        </Button>
      </nav>
    </section>
  )
}
