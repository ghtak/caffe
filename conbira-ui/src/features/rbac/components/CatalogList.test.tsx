import { screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { http, HttpResponse } from 'msw'
import { describe, expect, it } from 'vitest'
import { server } from '../../../../test/msw/server'
import { renderWithProviders } from '../../../../test/renderWithProviders'

function claims(roles: string[], permissions: string[]) {
  return http.get('*/api/auth/me', () => HttpResponse.json({
    code: 'OK',
    data: { exp: Math.floor(Date.now() / 1000) + 900, user: { roles, permissions } },
  }))
}

describe('contract', () => {
  it('비로그인 상태에서 관리 URL을 열면 로그인 화면을 표시한다', async () => {
    const { router } = renderWithProviders('/admin/roles')

    // 보호된 주소에서는 로그인 폼으로 이동한다.
    expect(await screen.findByRole('heading', { name: '로그인' })).toBeVisible()
    expect(router.state.location.pathname).toBe('/login')
    expect(screen.queryByRole('navigation', { name: '관리 메뉴' })).not.toBeInTheDocument()
  })

  it('rbac.read가 있는 일반 계정의 관리 URL 접근을 차단하고 메뉴를 숨긴다', async () => {
    server.use(claims(['user'], ['rbac.read']))
    const { router } = renderWithProviders('/admin/permissions', 'member-token')

    // 조회 권한이 있어도 admin 역할이 없으면 로그인된 홈으로 이동한다.
    expect(await screen.findByRole('heading', { name: '홈' })).toBeVisible()
    expect(router.state.location.pathname).toBe('/')
    expect(screen.getByRole('button', { name: '로그아웃' })).toBeVisible()
    expect(screen.queryByRole('link', { name: '관리' })).not.toBeInTheDocument()
  })

  it('admin 역할은 관리 홈에서 역할·권한 목록 조회 화면으로 이동할 수 있다', async () => {
    const user = userEvent.setup()
    server.use(
      claims(['user', 'admin'], ['rbac.read']),
      http.get('*/api/rbac/roles', () => HttpResponse.json({
        code: 'OK', data: [{ code: 'admin', name: '관리자', description: '서비스 관리' }],
      })),
      http.get('*/api/rbac/permissions', () => HttpResponse.json({
        code: 'OK', data: [{ code: 'rbac.read', name: '권한 설정 조회', description: '역할과 권한 조회' }],
      })),
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
