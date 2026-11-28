import { screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { http, HttpResponse } from 'msw'
import { describe, expect, it } from 'vitest'
import { server } from '../../../../test/msw/server'
import { renderWithProviders } from '../../../../test/renderWithProviders'

function session(permissions: string[]) {
  return http.get('*/api/auth/me', () =>
    HttpResponse.json({
      code: 'OK',
      data: { exp: Math.floor(Date.now() / 1000) + 900, user: { roles: ['user'], permissions } },
    }),
  )
}
function list() {
  return http.get('*/api/board/posts', () =>
    HttpResponse.json({ code: 'OK', data: { items: [], page: 1, page_size: 20, has_next: false } }),
  )
}
describe('contract', () => {
  it.each(['free', 'notice', 'data'] as const)(
    '게시판 %s 쓰기 권한으로 작성 후 목록으로 이동한다',
    async (board) => {
      let payload: unknown
      server.use(
        session([`board.${board}.write`]),
        list(),
        http.post('*/api/board/posts', async ({ request }) => {
          expect(request.headers.get('Authorization')).toBe('Bearer writer')
          payload = await request.json()
          return HttpResponse.json({
            code: 'OK',
            data: {
              id: 1,
              title: '새 글',
              board_slug: board,
              author_id: 1,
              view_count: 0,
              comment_count: 0,
              created_at: '2026-01-01T00:00:00Z',
            },
          })
        }),
      )
      const { router } = renderWithProviders(`/community/${board}`, 'writer')
      const user = userEvent.setup()
      await user.click(await screen.findByRole('link', { name: '글쓰기' }))
      await user.type(await screen.findByLabelText('제목'), '새 글')
      await user.type(screen.getByLabelText('본문'), '본문 내용')
      await user.click(screen.getByRole('button', { name: '등록' }))
      // 선택한 게시판과 입력을 전송하고 목록으로 복귀한다.
      expect(await screen.findByRole('heading', { name: '게시글 목록' })).toBeVisible()
      expect(payload).toEqual({ board_slug: board, title: '새 글', content: '본문 내용' })
      expect(router.state.location.pathname).toBe(`/community/${board}`)
    },
  )
  it('다른 게시판 쓰기 권한으로는 공지 작성 화면에 접근할 수 없다', async () => {
    server.use(session(['board.free.write']))
    renderWithProviders('/community/notice/new', 'writer')
    // 공지 쓰기 권한을 별도로 요구한다.
    expect(await screen.findByRole('alert')).toHaveTextContent('글을 작성할 권한이 없습니다.')
    expect(screen.queryByLabelText('제목')).not.toBeInTheDocument()
  })
  it('쓰기 권한이 없는 목록에서는 글쓰기 버튼을 숨긴다', async () => {
    server.use(session(['board.free.write']), list())
    renderWithProviders('/community/notice', 'writer')
    await screen.findByText('등록된 게시글이 없습니다.')
    // 타 게시판 권한으로 공지 글쓰기 버튼을 표시하지 않는다.
    expect(screen.queryByRole('link', { name: '글쓰기' })).not.toBeInTheDocument()
  })
})
describe('edge', () => {
  it('등록 실패 시 작성한 내용을 유지한다', async () => {
    server.use(
      session(['board.free.write']),
      http.post('*/api/board/posts', () => HttpResponse.json({}, { status: 500 })),
    )
    renderWithProviders('/community/free/new', 'writer')
    const user = userEvent.setup()
    await user.type(await screen.findByLabelText('제목'), '유지할 제목')
    await user.type(screen.getByLabelText('본문'), '유지할 본문')
    await user.click(screen.getByRole('button', { name: '등록' }))
    // 오류 안내와 입력을 유지해 재시도를 지원한다.
    expect(await screen.findByRole('alert')).toHaveTextContent('글을 등록하지 못했습니다.')
    expect(screen.getByLabelText('제목')).toHaveValue('유지할 제목')
    expect(screen.getByLabelText('본문')).toHaveValue('유지할 본문')
  })
})
