import { screen, waitFor } from '@testing-library/react'
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

function admin(permissions = ['auth.user.read', 'auth.user.manage']) {
  return http.get('*/api/auth/me', () =>
    HttpResponse.json({
      code: 'OK',
      data: {
        exp: Math.floor(Date.now() / 1000) + 900,
        user: { permissions },
      },
    }),
  )
}

describe('contract', () => {
  it('상태를 저장하면 서버의 변경된 상태를 다시 표시한다', async () => {
    let status = 'active'
    let submitted: unknown
    server.use(
      admin(),
      http.get('*/api/auth/users', () =>
        HttpResponse.json({
          code: 'OK',
          data: {
            items: [{ ...detail, status }],
            page: 1,
            page_size: 20,
            has_next: false,
          },
        }),
      ),
      http.get('*/api/auth/users/1', () =>
        HttpResponse.json({ code: 'OK', data: { ...detail, status: status } }),
      ),
      http.patch('*/api/auth/users/1/status', async ({ request }) => {
        submitted = await request.json()
        status = 'inactive'
        return HttpResponse.json({ code: 'OK' })
      }),
    )
    renderWithProviders('/admin/users', 'admin-token')
    const user = userEvent.setup()
    await user.click(await screen.findByRole('link', { name: '김하나' }))
    const select = await screen.findByLabelText('김하나 상태')
    select.focus()
    await user.selectOptions(select, 'inactive')
    await user.click(screen.getByRole('button', { name: '변경 확인' }))
    // 변경 요청을 전송하고 갱신된 상태를 표시한다.
    await waitFor(() => expect(screen.getByLabelText('김하나 상태')).toHaveValue('inactive'))
    expect(submitted).toEqual({ status: 'inactive' })
    expect(screen.getByLabelText('김하나 상태')).toHaveFocus()
    expect(screen.getByText('상태를 저장했습니다.')).toBeVisible()
    await user.click(screen.getByRole('link', { name: '사용자 목록으로' }))
    expect(await screen.findByRole('cell', { name: '비활성' })).toBeVisible()
    expect(screen.queryByLabelText('김하나 상태')).not.toBeInTheDocument()
  })
  it('저장 후 재조회가 실패해도 저장 성공을 안내한다', async () => {
    let changed = false
    server.use(
      admin(),
      http.get('*/api/auth/users/1', () =>
        changed
          ? HttpResponse.json({ code: 'FORBIDDEN' }, { status: 403 })
          : HttpResponse.json({ code: 'OK', data: detail }),
      ),
      http.patch('*/api/auth/users/1/status', () => {
        changed = true
        return HttpResponse.json({ code: 'OK' })
      }),
    )
    renderWithProviders('/admin/users/1', 'admin-token')
    await userEvent.setup().selectOptions(await screen.findByLabelText('김하나 상태'), 'inactive')
    await userEvent.setup().click(screen.getByRole('button', { name: '변경 확인' }))
    // 저장 결과와 상세 재조회 실패를 각각 안내한다.
    expect(await screen.findByRole('alert')).toHaveTextContent('사용자를 조회할 권한이 없습니다.')
    expect(screen.getByText('상태를 저장했습니다.')).toBeVisible()
  })
  it('조회 권한만 있으면 상태를 볼 수 있지만 변경할 수 없다', async () => {
    server.use(
      admin(['auth.user.read']),
      http.get('*/api/auth/users/1', () =>
        HttpResponse.json({ code: 'OK', data: { ...detail, status: 'active' } }),
      ),
    )
    renderWithProviders('/admin/users/1', 'admin-token')
    // 조회 전용 사용자는 상태를 읽지만 선택기를 보지 않는다.
    expect(await screen.findByText('김하나')).toBeVisible()
    expect(screen.queryByLabelText('김하나 상태')).not.toBeInTheDocument()
    expect(screen.getByText('활성')).toBeVisible()
  })
  it('상태 저장이 실패하면 기존 상태와 오류를 표시한다', async () => {
    server.use(
      admin(),
      http.get('*/api/auth/users/1', () =>
        HttpResponse.json({ code: 'OK', data: { ...detail, status: 'active' } }),
      ),
      http.patch('*/api/auth/users/1/status', () =>
        HttpResponse.json({ code: 'FORBIDDEN' }, { status: 403 }),
      ),
    )
    renderWithProviders('/admin/users/1', 'admin-token')
    const user = userEvent.setup()
    await user.selectOptions(await screen.findByLabelText('김하나 상태'), 'inactive')
    await user.click(screen.getByRole('button', { name: '변경 확인' }))
    // 실패를 알리고 실제 저장된 상태를 유지한다.
    expect(await screen.findByRole('alert')).toHaveTextContent('상태를 변경할 권한이 없습니다.')
    expect(screen.getByLabelText('김하나 상태')).toHaveValue('active')
  })
  it('변경 확인을 취소하면 기존 상태를 유지하고 요청하지 않는다', async () => {
    let requested = false
    server.use(
      admin(),
      http.get('*/api/auth/users/1', () =>
        HttpResponse.json({ code: 'OK', data: { ...detail, status: 'active' } }),
      ),
      http.patch('*/api/auth/users/1/status', () => {
        requested = true
        return HttpResponse.json({ code: 'OK' })
      }),
    )
    renderWithProviders('/admin/users/1', 'admin-token')
    await userEvent.setup().selectOptions(await screen.findByLabelText('김하나 상태'), 'inactive')
    await userEvent.setup().click(screen.getByRole('button', { name: '취소' }))
    // 취소하면 기존 상태를 표시하고 서버에 변경을 요청하지 않는다.
    expect(screen.getByLabelText('김하나 상태')).toHaveValue('active')
    expect(requested).toBe(false)
  })
})
