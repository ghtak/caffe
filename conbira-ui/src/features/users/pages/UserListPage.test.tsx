import { screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { http, HttpResponse } from 'msw'
import { describe, expect, it } from 'vitest'
import { server } from '../../../../test/msw/server'
import { renderWithProviders } from '../../../../test/renderWithProviders'

function admin(permissions = ['auth.user.read', 'auth.user.manage']) {
  return http.get('*/api/auth/me', () =>
    HttpResponse.json({
      code: 'OK',
      data: { exp: Math.floor(Date.now() / 1000) + 900, user: { roles: ['admin'], permissions } },
    }),
  )
}

const item = (id: number, nickname: string) => ({
  id,
  nickname,
  emails: ['user@example.com'],
  status: 'active',
  created_at: '2026-01-01T00:00:00Z',
})

describe('contract', () => {
  it('검색 조건과 페이지를 서버로 전달하고 응답을 표시한다', async () => {
    server.use(
      admin(),
      http.get('*/api/auth/users', ({ request }) => {
        expect(request.headers.get('Authorization')).toBe('Bearer admin-token')
        const query = new URL(request.url).searchParams
        const name = query.get('name')
        const email = query.get('email')
        const status = query.get('status')
        const page = Number(query.get('page'))
        const items =
          name === '김' && email === 'example' && status === 'active'
            ? [item(page, page === 1 ? '김하나' : '김둘')]
            : [item(1, '전체 사용자')]
        return HttpResponse.json({
          code: 'OK',
          data: { items, page, page_size: 20, has_next: name === '김' && page === 1 },
        })
      }),
    )
    renderWithProviders('/admin/users', 'admin-token')
    await screen.findByText('전체 사용자')
    const user = userEvent.setup()
    await user.type(screen.getByLabelText('이름'), '김')
    await user.type(screen.getByLabelText('이메일'), 'example')
    await user.selectOptions(screen.getByLabelText('상태'), 'active')
    await user.click(screen.getByRole('button', { name: '검색' }))
    // 서버가 조건에 맞춰 반환한 첫 페이지를 표시한다.
    expect(await screen.findByText('김하나')).toBeVisible()
    expect(screen.getByText('user@example.com')).toBeVisible()
    await user.click(screen.getByRole('button', { name: '다음' }))
    expect(await screen.findByText('김둘')).toBeVisible()
    expect(screen.getByText('2 페이지')).toBeVisible()
  })
})

describe('edge', () => {
  it('조회 권한이 없으면 목록에 접근할 수 없다', async () => {
    server.use(admin(['rbac.read']))
    renderWithProviders('/admin/users', 'admin-token')
    // 관리자로 진입하더라도 조회 권한 없이는 사용자 목록을 열지 않는다.
    expect(await screen.findByRole('alert')).toHaveTextContent(
      '사용자 목록을 조회할 권한이 없습니다.',
    )
  })

  it('조회 실패 후 재시도한 빈 목록을 안내한다', async () => {
    server.use(
      admin(),
      http.get('*/api/auth/users', () => HttpResponse.json({ code: 'FORBIDDEN' }, { status: 403 })),
    )
    renderWithProviders('/admin/users', 'admin-token')
    expect(await screen.findByRole('alert')).toHaveTextContent('목록을 조회할 권한이 없습니다.')
    server.use(
      http.get('*/api/auth/users', () =>
        HttpResponse.json({
          code: 'OK',
          data: { items: [], page: 1, page_size: 20, has_next: false },
        }),
      ),
    )
    await userEvent.setup().click(screen.getByRole('button', { name: '다시 시도' }))
    // 성공적으로 다시 조회하면 빈 목록 안내가 표시된다.
    expect(await screen.findByText('등록된 사용자가 없습니다.')).toBeVisible()
  })
})

describe('regression', () => {
  it('키보드로 페이지를 이동할 때 조회 중과 완료 후 포커스를 유지한다', async () => {
    let releasePage: () => void = () => {}
    const pageReady = new Promise<void>((resolve) => {
      releasePage = resolve
    })
    server.use(
      admin(),
      http.get('*/api/auth/users', async ({ request }) => {
        const page = Number(new URL(request.url).searchParams.get('page'))
        if (page === 2) await pageReady
        return HttpResponse.json({
          code: 'OK',
          data: {
            items: [item(page, page === 1 ? '첫 사용자' : '둘째 사용자')],
            page,
            page_size: 20,
            has_next: page === 1,
          },
        })
      }),
    )
    renderWithProviders('/admin/users', 'admin-token')
    await screen.findByText('첫 사용자')
    const user = userEvent.setup()
    const next = screen.getByRole('button', { name: '다음' })
    next.focus()
    await user.keyboard('{Enter}')
    // 로딩 중과 마지막 페이지 도착 후에도 같은 버튼에 초점이 남는다.
    expect(await screen.findByRole('status')).toHaveTextContent('목록을 불러오는 중')
    expect(next).toHaveFocus()
    expect(next).toHaveAttribute('aria-disabled', 'true')
    releasePage()
    expect(await screen.findByText('둘째 사용자')).toBeVisible()
    expect(next).toHaveFocus()
    expect(next).toHaveAttribute('aria-disabled', 'true')
  })
})
