import { screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { http, HttpResponse } from 'msw'
import { describe, expect, it } from 'vitest'
import { server } from '../../../../test/msw/server'
import { renderWithProviders } from '../../../../test/renderWithProviders'

const post = {
  id: 1,
  title: '게시글',
  author_id: 1,
  author_nickname: '작성자',
  board_slug: 'free',
  view_count: 0,
  comment_count: 0,
  created_at: '2026-01-01T00:00:00Z',
  content: '본문',
}
const root = {
  id: 10,
  post_id: 1,
  parent_id: null,
  author_id: 2,
  author_nickname: '독자',
  content: '기존 댓글',
  created_at: '2026-01-01T00:00:00Z',
  updated_at: '2026-01-01T00:00:00Z',
}

describe('contract', () => {
  it('본인 댓글만 수정하고 수정 표시와 작성일을 보여준다', async () => {
    let comments = [root, { ...root, id: 11, author_id: 1, content: '내 댓글' }]
    server.use(
      http.get('*/api/auth/me', () =>
        HttpResponse.json({
          data: { exp: Math.floor(Date.now() / 1000) + 900, user: { id: 1, permissions: [] } },
        }),
      ),
      http.get('*/api/board/posts/1', () => HttpResponse.json({ code: 'OK', data: post })),
      http.get('*/api/board/posts/1/comments', () =>
        HttpResponse.json({ code: 'OK', data: comments }),
      ),
      http.put('*/api/board/comments/11', async ({ request }) => {
        expect(request.headers.get('Authorization')).toBe('Bearer writer')
        const body = await request.json()
        expect(body).toEqual({ content: '수정한 댓글' })
        comments = comments.map((comment) =>
          comment.id === 11
            ? { ...comment, content: '수정한 댓글', updated_at: '2026-01-02T00:00:00Z' }
            : comment,
        )
        return HttpResponse.json({ code: 'OK', data: {} })
      }),
    )
    renderWithProviders('/community/posts/1', 'writer')
    const user = userEvent.setup()
    const own = (await screen.findByText('내 댓글')).closest('li')!
    const other = screen.getByText('기존 댓글').closest('li')!
    expect(within(other).queryByRole('button', { name: '수정' })).not.toBeInTheDocument()
    expect(within(own).getByText(new Date(root.created_at).toLocaleString('ko-KR'))).toBeVisible()
    expect(within(own).queryByText('(수정됨)')).not.toBeInTheDocument()
    await user.click(within(own).getByRole('button', { name: '수정' }))
    const input = within(own).getByLabelText('댓글 수정')
    expect(input).toHaveValue('내 댓글')
    await user.clear(input)
    await user.type(input, '수정한 댓글')
    await user.click(within(own).getByRole('button', { name: '저장' }))
    expect(await screen.findByText('수정한 댓글')).toBeVisible()
    expect(within(own).getByText('(수정됨)')).toBeVisible()
  })
  it('댓글과 선택한 댓글의 답글을 등록하고 목록에 반영한다', async () => {
    const comments: Array<Omit<typeof root, 'parent_id'> & { parent_id: number | null }> = [root]
    const requests: unknown[] = []
    server.use(
      http.get('*/api/auth/me', () =>
        HttpResponse.json({
          data: { exp: Math.floor(Date.now() / 1000) + 900, user: { id: 1, permissions: [] } },
        }),
      ),
      http.get('*/api/board/posts/1', () => HttpResponse.json({ code: 'OK', data: post })),
      http.get('*/api/board/posts/1/comments', () =>
        HttpResponse.json({ code: 'OK', data: comments }),
      ),
      http.post('*/api/board/posts/1/comments', async ({ request }) => {
        expect(request.headers.get('Authorization')).toBe('Bearer writer')
        const body = (await request.json()) as { content: string; parent_id?: number }
        requests.push(body)
        comments.push({
          ...root,
          id: 10 + comments.length,
          parent_id: body.parent_id ?? null,
          content: body.content,
        })
        return HttpResponse.json({ code: 'OK', data: {} })
      }),
    )
    renderWithProviders('/community/posts/1', 'writer')
    const user = userEvent.setup()
    await user.type(await screen.findByLabelText('댓글'), '새 댓글')
    await user.click(screen.getByRole('button', { name: '등록' }))
    expect(await screen.findByText('새 댓글')).toBeVisible()
    const existing = screen
      .getAllByRole('listitem')
      .find((item) => within(item).queryByText('기존 댓글'))!
    await user.click(within(existing).getByRole('button', { name: '답글달기' }))
    await user.type(within(existing).getByLabelText('답글'), '새 답글')
    await user.click(within(existing).getByRole('button', { name: '등록' }))
    expect(await screen.findByText('새 답글')).toBeVisible()
    expect(requests).toEqual([{ content: '새 댓글' }, { content: '새 답글', parent_id: 10 }])
  })
})

describe('edge', () => {
  it('비로그인 사용자는 댓글을 읽고 로그인 안내를 본다', async () => {
    server.use(
      http.get('*/api/board/posts/1', () => HttpResponse.json({ code: 'OK', data: post })),
      http.get('*/api/board/posts/1/comments', () =>
        HttpResponse.json({ code: 'OK', data: [root] }),
      ),
    )
    renderWithProviders('/community/posts/1')
    expect(await screen.findByText('기존 댓글')).toBeVisible()
    expect(
      within(screen.getByRole('region', { name: '댓글 목록' })).getByRole('link', {
        name: '로그인',
      }),
    ).toBeVisible()
    expect(screen.getByLabelText('댓글')).toBeDisabled()
  })
})
