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
  it('자료실 파일을 업로드하고 게시글에 연결한다', async () => {
    const file = 'dcb68ff6-4ed1-4a56-9788-b35d887bf91b'
    let payload: unknown
    server.use(
      session(['board.data.write']),
      list(),
      http.post('*/api/board/posts/attachments/upload', () =>
        HttpResponse.json({
          data: { id: file, upload_url: 'https://storage.example/upload', headers: [] },
        }),
      ),
      http.put('https://storage.example/upload', () => new HttpResponse(null, { status: 200 })),
      http.post('*/api/board/posts', async ({ request }) => {
        payload = await request.json()
        return HttpResponse.json({
          code: 'OK',
          data: {
            id: 1,
            title: '제목',
            board_slug: 'data',
            author_id: 1,
            view_count: 0,
            comment_count: 0,
            created_at: '2026-01-01T00:00:00Z',
          },
        })
      }),
    )
    renderWithProviders('/community/data/new', 'writer')
    const user = userEvent.setup()
    await user.type(await screen.findByLabelText('제목'), '제목')
    await user.type(screen.getByLabelText('본문'), '본문')
    await user.upload(screen.getByLabelText('파일 선택'), new File(['content'], '자료.txt'))
    await user.click(screen.getByRole('button', { name: '등록' }))
    expect(await screen.findByRole('heading', { name: '게시글 목록' })).toBeVisible()
    expect(payload).toEqual({
      board_slug: 'data',
      title: '제목',
      content: '본문',
      attachments: { attach_file_ids: [file], detach_file_ids: [] },
    })
  })
  it('자료실에서 파일을 추가하고 제거할 수 있다', async () => {
    server.use(session(['board.data.write']))
    renderWithProviders('/community/data/new', 'writer')
    const user = userEvent.setup()
    const input = await screen.findByLabelText('파일 선택')
    await user.upload(input, [new File(['a'], '첫째.txt'), new File(['b'], '둘째.txt')])
    expect(screen.getByText('첫째.txt')).toBeVisible()
    expect(screen.getByRole('button', { name: '등록' })).toBeEnabled()
    await user.click(screen.getByRole('button', { name: '첫째.txt 삭제' }))
    expect(screen.queryByText('첫째.txt')).not.toBeInTheDocument()
    expect(screen.getByText('둘째.txt')).toBeVisible()
    await user.click(screen.getByRole('button', { name: '둘째.txt 삭제' }))
    expect(screen.getByRole('button', { name: '등록' })).toBeEnabled()
  })
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
