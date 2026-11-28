import { screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { http, HttpResponse } from 'msw'
import { describe, expect, it } from 'vitest'
import { server } from '../../../../test/msw/server'
import { renderWithProviders } from '../../../../test/renderWithProviders'

const post = {
  id: 1,
  title: '상세 제목',
  author_id: 1,
  author_nickname: '작성자',
  board_slug: 'free',
  view_count: 0,
  comment_count: 0,
  created_at: '2026-01-01T00:00:00Z',
}
describe('contract', () => {
  it('작성자가 제목과 본문을 수정하면 상세 화면에 반영한다', async () => {
    let payload: unknown
    server.use(
      http.get('*/api/auth/me', () =>
        HttpResponse.json({
          data: { exp: Math.floor(Date.now() / 1000) + 900, user: { id: 1, permissions: [] } },
        }),
      ),
      http.get('*/api/board/posts/1', () =>
        HttpResponse.json({ code: 'OK', data: { ...post, content: '기존 본문' } }),
      ),
      http.put('*/api/board/posts/1', async ({ request }) => {
        expect(request.headers.get('Authorization')).toBe('Bearer writer')
        payload = await request.json()
        return HttpResponse.json({ code: 'OK', data: { ...post, title: '수정 제목' } })
      }),
    )
    renderWithProviders('/community/posts/1', 'writer')
    const user = userEvent.setup()
    await user.click(await screen.findByRole('button', { name: '수정' }))
    await user.clear(screen.getByLabelText('제목'))
    await user.type(screen.getByLabelText('제목'), '수정 제목')
    await user.clear(screen.getByLabelText('본문'))
    await user.type(screen.getByLabelText('본문'), '수정 본문')
    await user.click(screen.getByRole('button', { name: '저장' }))
    expect(await screen.findByRole('heading', { name: '수정 제목' })).toBeVisible()
    expect(screen.getByRole('region', { name: '본문' })).toHaveTextContent('수정 본문')
    expect(payload).toEqual({ title: '수정 제목', content: '수정 본문' })
  })
  it('비로그인 사용자에게 첨부 목록과 로그인 안내를 표시한다', async () => {
    server.use(
      http.get('*/api/board/posts/1', () =>
        HttpResponse.json({
          code: 'OK',
          data: {
            ...post,
            board_slug: 'data',
            content: '본문',
            attachments: [
              {
                id: 'dcb68ff6-4ed1-4a56-9788-b35d887bf91b',
                original_filename: '자료.txt',
              },
            ],
          },
        }),
      ),
    )
    renderWithProviders('/community/posts/1')
    expect(await screen.findByText('자료.txt')).toBeVisible()
    expect(screen.getByText('로그인 후 다운로드')).toBeVisible()
    expect(screen.queryByRole('button', { name: '다운로드' })).not.toBeInTheDocument()
  })
  it('다른 사용자의 게시글에는 수정 버튼을 표시하지 않는다', async () => {
    server.use(
      http.get('*/api/auth/me', () =>
        HttpResponse.json({
          data: { exp: Math.floor(Date.now() / 1000) + 900, user: { id: 2, permissions: [] } },
        }),
      ),
      http.get('*/api/board/posts/1', () =>
        HttpResponse.json({ code: 'OK', data: { ...post, content: '본문' } }),
      ),
    )
    renderWithProviders('/community/posts/1', 'reader')
    await screen.findByRole('button', { name: '로그아웃' })
    await screen.findByRole('heading', { name: '상세 제목' })
    expect(screen.queryByRole('button', { name: '수정' })).not.toBeInTheDocument()
  })
  it('목록 제목에서 상세를 열고 본문을 일반 텍스트로 표시한 후 목록으로 돌아간다', async () => {
    const content = '# 제목 아님\n<script>alert(1)</script>'
    server.use(
      http.get('*/api/board/posts', () =>
        HttpResponse.json({
          code: 'OK',
          data: { items: [post], page: 1, page_size: 20, has_next: false },
        }),
      ),
      http.get('*/api/board/posts/1', () =>
        HttpResponse.json({ code: 'OK', data: { ...post, content } }),
      ),
    )
    renderWithProviders('/community/free')
    const user = userEvent.setup()
    await user.click(await screen.findByRole('link', { name: '상세 제목' }))
    // 원문 기호와 HTML 태그를 그대로 표시한다.
    expect(await screen.findByRole('heading', { name: '상세 제목' })).toBeVisible()
    expect(screen.getByRole('region', { name: '본문' })).toHaveTextContent(
      '# 제목 아님 <script>alert(1)</script>',
    )
    expect(screen.queryByRole('heading', { name: '제목 아님' })).not.toBeInTheDocument()
    await user.click(screen.getByRole('link', { name: '목록으로' }))
    expect(await screen.findByRole('heading', { name: '게시글 목록' })).toBeVisible()
  })
})
describe('edge', () => {
  it('없는 글은 오류 안내와 목록 이동을 표시한다', async () => {
    server.use(http.get('*/api/board/posts/999', () => HttpResponse.json({}, { status: 404 })))
    renderWithProviders('/community/posts/999')
    expect(await screen.findByRole('alert')).toHaveTextContent('게시글을 찾을 수 없습니다.')
    expect(screen.getByRole('link', { name: '목록으로' })).toBeVisible()
  })
})
