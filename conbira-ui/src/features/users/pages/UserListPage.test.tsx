import { screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { http, HttpResponse } from 'msw'
import { describe, expect, it, vi } from 'vitest'
import { server } from '../../../../test/msw/server'
import { renderWithProviders } from '../../../../test/renderWithProviders'

function admin(permissions = ['auth.user.read', 'auth.user.manage']) {
  return http.get('*/api/auth/me', () => HttpResponse.json({
    code: 'OK', data: { exp: Math.floor(Date.now() / 1000) + 900, user: { roles: ['admin'], permissions } },
  }))
}

const item = (id: number, nickname: string) => ({ id, nickname, emails: ['user@example.com'], status: 'active', created_at: '2026-01-01T00:00:00Z' })

describe('contract', () => {
  it('상태를 저장하면 서버의 변경된 상태를 다시 표시한다', async () => {
    let status = 'active'
    let submitted: unknown
    server.use(admin(),
      http.get('*/api/auth/users', () => HttpResponse.json({ code: 'OK', data: { items: [{ ...item(1, '김하나'), status }], page: 1, page_size: 20, has_next: false } })),
      http.patch('*/api/auth/users/1/status', async ({ request }) => {
        submitted = await request.json()
        status = 'inactive'
        return HttpResponse.json({ code: 'OK' })
      }),
    )
    renderWithProviders('/admin/users', 'admin-token')
    const select = await screen.findByLabelText('김하나 상태')
    const user = userEvent.setup()
    const confirm = vi.spyOn(window, 'confirm').mockReturnValueOnce(true)
    select.focus()
    await user.selectOptions(select, 'inactive')
    // 변경 요청을 전송하고 갱신된 상태를 표시한다.
    await waitFor(() => expect(screen.getByLabelText('김하나 상태')).toHaveValue('inactive'))
    expect(submitted).toEqual({ status: 'inactive' })
    expect(screen.getByLabelText('김하나 상태')).toHaveFocus()
    expect(screen.getByText('김하나님의 상태를 저장했습니다.')).toBeVisible()
    confirm.mockRestore()
  })

  it('검색 조건과 페이지를 서버로 전달하고 응답을 표시한다', async () => {
    server.use(admin(), http.get('*/api/auth/users', ({ request }) => {
      expect(request.headers.get('Authorization')).toBe('Bearer admin-token')
      const query = new URL(request.url).searchParams
      const name = query.get('name')
      const email = query.get('email')
      const status = query.get('status')
      const page = Number(query.get('page'))
      const items = name === '김' && email === 'example' && status === 'active' ? [item(page, page === 1 ? '김하나' : '김둘')] : [item(1, '전체 사용자')]
      return HttpResponse.json({ code: 'OK', data: { items, page, page_size: 20, has_next: name === '김' && page === 1 } })
    }))
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
  it('저장 후 필터에서 제외된 사용자의 포커스를 목록 제목으로 이동한다', async () => {
    let changed = false
    server.use(admin(),
      http.get('*/api/auth/users', () => HttpResponse.json({ code: 'OK', data: { items: changed ? [] : [item(1, '김하나')], page: 1, page_size: 20, has_next: false } })),
      http.patch('*/api/auth/users/1/status', () => { changed = true; return HttpResponse.json({ code: 'OK' }) }),
    )
    renderWithProviders('/admin/users', 'admin-token')
    const select = await screen.findByLabelText('김하나 상태')
    const confirm = vi.spyOn(window, 'confirm').mockReturnValueOnce(true)
    select.focus()
    await userEvent.setup().selectOptions(select, 'inactive')
    // 변경된 사용자가 목록에서 사라지면 안정적인 제목으로 포커스를 옮긴다.
    await waitFor(() => expect(screen.getByRole('heading', { name: '사용자 목록' })).toHaveFocus())
    expect(screen.getByText('김하나님의 상태를 저장했습니다.')).toBeVisible()
    confirm.mockRestore()
  })

  it('저장 후 재조회가 실패해도 저장 성공을 안내한다', async () => {
    let changed = false
    server.use(admin(),
      http.get('*/api/auth/users', () => changed ? HttpResponse.json({ code: 'FORBIDDEN' }, { status: 403 }) : HttpResponse.json({ code: 'OK', data: { items: [item(1, '김하나')], page: 1, page_size: 20, has_next: false } })),
      http.patch('*/api/auth/users/1/status', () => { changed = true; return HttpResponse.json({ code: 'OK' }) }),
    )
    renderWithProviders('/admin/users', 'admin-token')
    const confirm = vi.spyOn(window, 'confirm').mockReturnValueOnce(true)
    await userEvent.setup().selectOptions(await screen.findByLabelText('김하나 상태'), 'inactive')
    // 저장 결과와 목록 재조회 실패를 각각 안내한다.
    expect(await screen.findByRole('alert')).toHaveTextContent('목록을 조회할 권한이 없습니다.')
    expect(screen.getByText('김하나님의 상태를 저장했습니다.')).toBeVisible()
    confirm.mockRestore()
  })

  it('조회 권한만 있으면 상태를 볼 수 있지만 변경할 수 없다', async () => {
    server.use(admin(['auth.user.read']), http.get('*/api/auth/users', () => HttpResponse.json({ code: 'OK', data: { items: [item(1, '김하나')], page: 1, page_size: 20, has_next: false } })))
    renderWithProviders('/admin/users', 'admin-token')
    // 조회 전용 사용자는 상태를 읽지만 선택기를 보지 않는다.
    expect(await screen.findByText('김하나')).toBeVisible()
    expect(screen.queryByLabelText('김하나 상태')).not.toBeInTheDocument()
    expect(screen.getByRole('cell', { name: '활성' })).toBeVisible()
  })

  it('조회 권한이 없으면 목록에 접근할 수 없다', async () => {
    server.use(admin([]))
    renderWithProviders('/admin/users', 'admin-token')
    // 관리자로 진입하더라도 조회 권한 없이는 사용자 목록을 열지 않는다.
    expect(await screen.findByRole('alert')).toHaveTextContent('사용자 목록을 조회할 권한이 없습니다.')
  })

  it('상태 저장이 실패하면 기존 상태와 오류를 표시한다', async () => {
    server.use(admin(),
      http.get('*/api/auth/users', () => HttpResponse.json({ code: 'OK', data: { items: [item(1, '김하나')], page: 1, page_size: 20, has_next: false } })),
      http.patch('*/api/auth/users/1/status', () => HttpResponse.json({ code: 'FORBIDDEN' }, { status: 403 })),
    )
    renderWithProviders('/admin/users', 'admin-token')
    const user = userEvent.setup()
    const confirm = vi.spyOn(window, 'confirm').mockReturnValueOnce(true)
    const alert = vi.spyOn(window, 'alert').mockImplementation(() => {})
    await user.selectOptions(await screen.findByLabelText('김하나 상태'), 'inactive')
    // 실패를 알리고 실제 저장된 상태를 유지한다.
    await waitFor(() => expect(alert).toHaveBeenCalledWith('상태를 변경할 권한이 없습니다.'))
    expect(screen.getByLabelText('김하나 상태')).toHaveValue('active')
    confirm.mockRestore()
    alert.mockRestore()
  })

  it('변경 확인을 취소하면 기존 상태를 유지하고 요청하지 않는다', async () => {
    let requested = false
    server.use(admin(),
      http.get('*/api/auth/users', () => HttpResponse.json({ code: 'OK', data: { items: [item(1, '김하나')], page: 1, page_size: 20, has_next: false } })),
      http.patch('*/api/auth/users/1/status', () => { requested = true; return HttpResponse.json({ code: 'OK' }) }),
    )
    renderWithProviders('/admin/users', 'admin-token')
    const confirm = vi.spyOn(window, 'confirm').mockReturnValueOnce(false)
    await userEvent.setup().selectOptions(await screen.findByLabelText('김하나 상태'), 'inactive')
    // 취소하면 기존 상태를 표시하고 서버에 변경을 요청하지 않는다.
    expect(screen.getByLabelText('김하나 상태')).toHaveValue('active')
    expect(requested).toBe(false)
    confirm.mockRestore()
  })

  it('조회 실패 후 재시도한 빈 목록을 안내한다', async () => {
    server.use(admin(), http.get('*/api/auth/users', () => HttpResponse.json({ code: 'FORBIDDEN' }, { status: 403 })))
    renderWithProviders('/admin/users', 'admin-token')
    expect(await screen.findByRole('alert')).toHaveTextContent('목록을 조회할 권한이 없습니다.')
    server.use(http.get('*/api/auth/users', () => HttpResponse.json({ code: 'OK', data: { items: [], page: 1, page_size: 20, has_next: false } })))
    await userEvent.setup().click(screen.getByRole('button', { name: '다시 시도' }))
    // 성공적으로 다시 조회하면 빈 목록 안내가 표시된다.
    expect(await screen.findByText('등록된 사용자가 없습니다.')).toBeVisible()
  })
})

describe('regression', () => {
  it('키보드로 페이지를 이동할 때 조회 중과 완료 후 포커스를 유지한다', async () => {
    let releasePage: () => void = () => {}
    const pageReady = new Promise<void>((resolve) => { releasePage = resolve })
    server.use(admin(), http.get('*/api/auth/users', async ({ request }) => {
      const page = Number(new URL(request.url).searchParams.get('page'))
      if (page === 2) await pageReady
      return HttpResponse.json({ code: 'OK', data: { items: [item(page, page === 1 ? '첫 사용자' : '둘째 사용자')], page, page_size: 20, has_next: page === 1 } })
    }))
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
