import { screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { http, HttpResponse } from 'msw'
import { describe, expect, it } from 'vitest'
import { server } from '../../../../test/msw/server'
import { renderWithProviders } from '../../../../test/renderWithProviders'

function claims(roles: string[], permissions: string[]) {
  return http.get('*/api/auth/me', () =>
    HttpResponse.json({
      code: 'OK',
      data: { exp: Math.floor(Date.now() / 1000) + 900, user: { roles, permissions } },
    }),
  )
}

describe('contract', () => {
  it('비로그인 상태에서 관리 URL을 열면 로그인 화면을 표시한다', async () => {
    const { router } = renderWithProviders('/admin/roles')

    // 보호된 주소에서는 로그인 폼으로 이동한다.
    expect(await screen.findByRole('heading', { name: '로그인' })).toBeVisible()
    expect(router.state.location.pathname).toBe('/login')
    expect(screen.queryByRole('navigation', { name: '관리 메뉴' })).not.toBeInTheDocument()
  })

  it('rbac.read가 있는 일반 계정은 권한 관리에 접근할 수 있다', async () => {
    server.use(
      claims(['user'], ['rbac.read']),
      http.get('*/api/rbac/permissions', () => HttpResponse.json({ code: 'OK', data: [] })),
    )
    const { router } = renderWithProviders('/admin/permissions', 'member-token')

    // 역할이 없어도 조회 권한으로 메뉴와 화면을 이용한다.
    expect(await screen.findByRole('heading', { name: '권한 관리' })).toBeVisible()
    expect(router.state.location.pathname).toBe('/admin/permissions')
    expect(screen.getByRole('link', { name: '관리' })).toBeVisible()
    expect(screen.queryByRole('link', { name: '사용자 목록' })).not.toBeInTheDocument()
  })

  it('관리 메뉴 권한이 없는 admin은 관리 영역에 접근할 수 없다', async () => {
    server.use(claims(['admin'], []))
    renderWithProviders('/admin', 'admin-token')

    // 역할만으로 관리 메뉴를 표시하지 않는다.
    expect(await screen.findByRole('heading', { name: '홈' })).toBeVisible()
    expect(screen.queryByRole('link', { name: '관리' })).not.toBeInTheDocument()
  })

  it('사용자 조회 권한만 있으면 역할 관리 직접 접근도 거부한다', async () => {
    server.use(claims(['user'], ['auth.user.read']))
    renderWithProviders('/admin/roles', 'member-token')

    // 다른 관리 메뉴의 권한은 역할 목록 접근을 허용하지 않는다.
    expect(await screen.findByRole('alert')).toHaveTextContent('목록을 조회할 권한이 없습니다.')
    expect(screen.queryByRole('link', { name: '역할 관리' })).not.toBeInTheDocument()
  })

  it('사용자 조회 권한이 없으면 역할의 사용자 설정을 표시하지 않는다', async () => {
    server.use(
      claims(['user'], ['rbac.read', 'rbac.user_role.manage']),
      http.get('*/api/rbac/roles/1', () =>
        HttpResponse.json({
          code: 'OK',
          data: { id: 1, code: 'operator', name: '운영자', description: '' },
        }),
      ),
      http.get('*/api/rbac/roles/1/permissions', () => HttpResponse.json({ code: 'OK', data: [] })),
      http.get('*/api/rbac/permissions', () => HttpResponse.json({ code: 'OK', data: [] })),
    )
    renderWithProviders('/admin/roles/1', 'member-token')

    // 역할 사용자 조회에 필요한 두 권한이 모두 있어야 탭을 연다.
    expect(await screen.findByRole('heading', { name: '운영자' })).toBeVisible()
    expect(screen.queryByRole('button', { name: '사용자 설정' })).not.toBeInTheDocument()
  })

  it('admin 역할은 관리 홈에서 역할·권한 목록 조회 화면으로 이동할 수 있다', async () => {
    const user = userEvent.setup()
    server.use(
      claims(['user', 'admin'], ['rbac.read']),
      http.get('*/api/rbac/roles', () =>
        HttpResponse.json({
          code: 'OK',
          data: [{ id: 1, code: 'admin', name: '관리자', description: '서비스 관리' }],
        }),
      ),
      http.get('*/api/rbac/permissions', () =>
        HttpResponse.json({
          code: 'OK',
          data: [
            { id: 1, code: 'rbac.read', name: '권한 설정 조회', description: '역할과 권한 조회' },
          ],
        }),
      ),
    )
    const { router } = renderWithProviders('/', 'admin-token')
    await user.click(await screen.findByRole('link', { name: '관리' }))
    await screen.findByRole('heading', { name: '관리 홈' })
    const navigation = within(screen.getByRole('navigation', { name: '관리 메뉴' }))
    // 관리 홈에서는 관리 홈 링크가 현재 위치로 표시된다.
    expect(navigation.getByRole('link', { current: 'page' })).toHaveTextContent('관리 홈')

    await user.click(navigation.getByRole('link', { name: '역할 관리' }))

    // 역할 목록이 열리고 관리 메뉴의 현재 위치가 역할 관리로 바뀐다.
    expect(await screen.findByRole('table', { name: '역할 목록' })).toHaveTextContent('관리자')
    expect(router.state.location.pathname).toBe('/admin/roles')
    expect(navigation.getByRole('link', { current: 'page' })).toHaveTextContent('역할 관리')

    await user.click(navigation.getByRole('link', { name: '권한 관리' }))

    // 권한 목록이 열리고 관리 메뉴의 현재 위치가 권한 관리로 바뀐다.
    expect(await screen.findByRole('table', { name: '권한 목록' })).toHaveTextContent('rbac.read')
    expect(router.state.location.pathname).toBe('/admin/permissions')
    expect(navigation.getByRole('link', { current: 'page' })).toHaveTextContent('권한 관리')
  })
})

describe('edge', () => {
  it('서버가 목록 조회를 거부하면 재시도를 제공하고 빈 응답을 안내한다', async () => {
    const user = userEvent.setup()
    server.use(
      claims(['admin'], ['rbac.read']),
      http.get('*/api/rbac/roles', () => HttpResponse.json({ code: 'FORBIDDEN' }, { status: 403 })),
    )
    renderWithProviders('/admin/roles', 'admin-token')

    // 목록 API의 거부 응답을 화면에 표시한다.
    expect(await screen.findByRole('alert')).toHaveTextContent('목록을 조회할 권한이 없습니다.')

    server.use(http.get('*/api/rbac/roles', () => HttpResponse.json({ code: 'OK', data: [] })))
    await user.click(screen.getByRole('button', { name: '다시 시도' }))

    // 재조회 결과가 비어 있으면 오류 대신 빈 목록 안내를 표시한다.
    expect(await screen.findByText('등록된 역할이 없습니다.')).toBeVisible()
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  })
})
