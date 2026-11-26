import { screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { http, HttpResponse } from 'msw'
import { describe, expect, it } from 'vitest'
import { server } from '../../../../test/msw/server'
import { renderWithProviders } from '../../../../test/renderWithProviders'

const detail = {
  id: 1,
  nickname: '김하나',
  emails: [],
  status: 'active',
  created_at: '2026-01-01T00:00:00Z',
  updated_at: '2026-02-01T00:00:00Z',
}
const session = http.get('*/api/auth/me', () =>
  HttpResponse.json({
    code: 'OK',
    data: { exp: Math.floor(Date.now() / 1000) + 900, user: { permissions: ['auth.user.read'] } },
  }),
)

describe('contract', () => {
  it('목록 링크에서 상세의 최신 서버 정보를 표시한다', async () => {
    server.use(
      session,
      http.get('*/api/auth/users', () =>
        HttpResponse.json({
          code: 'OK',
          data: { items: [detail], page: 1, page_size: 20, has_next: false },
        }),
      ),
      http.get('*/api/auth/users/1', ({ request }) => {
        expect(request.headers.get('Authorization')).toBe('Bearer admin-token')
        return HttpResponse.json({ code: 'OK', data: { ...detail, nickname: '최신 닉네임' } })
      }),
    )
    renderWithProviders('/admin/users', 'admin-token')
    await userEvent.setup().click(await screen.findByRole('link', { name: '김하나' }))
    // 목록 복사본 대신 상세 응답과 빈 이메일 안내를 표시한다.
    expect(await screen.findByText('최신 닉네임')).toBeVisible()
    expect(screen.getByText('연결된 이메일이 없습니다.')).toBeVisible()
    expect(screen.queryByLabelText('최신 닉네임 상태')).not.toBeInTheDocument()
  })

  it('직접 접근에서 로딩 후 상세를 표시한다', async () => {
    let release = () => {}
    const ready = new Promise<void>((resolve) => {
      release = resolve
    })
    server.use(
      session,
      http.get('*/api/auth/users/1', async () => {
        await ready
        return HttpResponse.json({ code: 'OK', data: detail })
      }),
    )
    renderWithProviders('/admin/users/1', 'admin-token')
    // 직접 접근도 서버 응답을 기다린다.
    expect(await screen.findByText('사용자 정보를 불러오는 중…')).toHaveAttribute('role', 'status')
    release()
    expect(await screen.findByText('김하나')).toBeVisible()
  })
})

describe('edge', () => {
  it.each([
    [403, '사용자를 조회할 권한이 없습니다.'],
    [404, '사용자를 찾을 수 없습니다.'],
    [500, '사용자 정보를 불러오지 못했습니다.'],
  ] as const)('%i 응답을 안내한다', async (status, message) => {
    server.use(
      session,
      http.get('*/api/auth/users/1', () => HttpResponse.json({ code: 'ERROR' }, { status })),
    )
    renderWithProviders('/admin/users/1', 'admin-token')
    // 실패 종류에 맞는 안내를 표시한다.
    expect(await screen.findByRole('alert')).toHaveTextContent(message)
  })

  it('재시도 성공 시 오류를 상세로 대체한다', async () => {
    server.use(
      session,
      http.get('*/api/auth/users/1', () =>
        HttpResponse.json({ code: 'INTERNAL' }, { status: 500 }),
      ),
    )
    renderWithProviders('/admin/users/1', 'admin-token')
    await screen.findByRole('alert')
    server.use(
      http.get('*/api/auth/users/1', () => HttpResponse.json({ code: 'OK', data: detail })),
    )
    await userEvent.setup().click(screen.getByRole('button', { name: '다시 시도' }))
    // 재시도 응답으로 오류 화면을 대체한다.
    expect(await screen.findByText('김하나')).toBeVisible()
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  })
})
