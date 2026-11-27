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
