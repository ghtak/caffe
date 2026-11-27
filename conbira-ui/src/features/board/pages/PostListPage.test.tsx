import { screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { http, HttpResponse } from 'msw'
import { describe, expect, it } from 'vitest'
import { server } from '../../../../test/msw/server'
import { renderWithProviders } from '../../../../test/renderWithProviders'

function posts() {
  return http.get('*/api/board/posts', ({ request }) => {
    expect(request.headers.has('Authorization')).toBe(false)
    const query = new URL(request.url).searchParams
    const board = query.get('board_slug')
    const page = Number(query.get('page'))
    const search = query.get('search')
    if (search) expect(query.get('author_id')).toBe('42')
    return HttpResponse.json({
      code: 'OK',
      data: {
        items: [
          {
            id: page,
            title: `${board} ${search || '전체'} ${page}`,
            board_slug: board,
            author_id: 42,
            author_nickname: '작성자닉네임',
            view_count: 0,
            comment_count: 0,
            created_at: '2026-01-01T00:00:00Z',
          },
        ],
        page,
        page_size: 20,
        has_next: page === 1,
      },
    })
  })
}

describe('contract', () => {
  it('비로그인 상태에서 커뮤니티 검색·페이지 이동과 게시판 전환을 지원한다', async () => {
    server.use(posts())
    renderWithProviders('/')
    const user = userEvent.setup()
    await user.click(await screen.findByRole('link', { name: '커뮤니티' }))
    expect(await screen.findByText('notice 전체 1')).toBeVisible()
    expect(screen.getByRole('cell', { name: '작성자닉네임' })).toBeVisible()
    expect(screen.getByRole('cell', { name: 'notice 전체 1[0]' })).toBeVisible()
    await user.click(screen.getByRole('link', { name: '자유게시판' }))
    await screen.findByText('free 전체 1')
    await user.type(screen.getByLabelText('제목·본문'), '검색어')
    await user.type(screen.getByLabelText('작성자 ID'), '42')
    await user.click(screen.getByRole('button', { name: '검색' }))
    expect(await screen.findByText('free 검색어 1')).toBeVisible()
    await user.click(screen.getByRole('button', { name: '다음' }))
    expect(await screen.findByText('free 검색어 2')).toBeVisible()
    await user.click(screen.getByRole('link', { name: '공지' }))
    // 게시판을 전환하면 첫 페이지와 빈 검색 입력으로 시작한다.
    expect(await screen.findByText('notice 전체 1')).toBeVisible()
    expect(screen.getByLabelText('제목·본문')).toHaveValue('')
    expect(screen.getByText('1 페이지')).toBeVisible()
  })
})

describe('edge', () => {
  it('조회 오류에서 재시도하고 빈 목록을 안내한다', async () => {
    server.use(http.get('*/api/board/posts', () => HttpResponse.json({}, { status: 500 })))
    renderWithProviders('/community/notice')
    expect(await screen.findByRole('alert')).toHaveTextContent('목록을 불러오지 못했습니다.')
    server.use(
      http.get('*/api/board/posts', () =>
        HttpResponse.json({
          code: 'OK',
          data: { items: [], page: 1, page_size: 20, has_next: false },
        }),
      ),
    )
    await userEvent.setup().click(screen.getByRole('button', { name: '다시 시도' }))
    // 재조회 성공 시 빈 결과를 표시한다.
    expect(await screen.findByText('등록된 게시글이 없습니다.')).toBeVisible()
  })
})
