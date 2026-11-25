import { screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { http, HttpResponse } from 'msw'
import { describe, expect, it } from 'vitest'
import { server } from '../../../../test/msw/server'
import { renderWithProviders } from '../../../../test/renderWithProviders'

describe('contract', () => {
  it('역할 상세에서 권한과 사용자를 단건으로 추가하고 결과를 표시한다', async () => {
    const user = userEvent.setup()
    const role = { id: 1, code: 'operator', name: '운영자', description: '운영' }
    const permission = { id: 2, code: 'item.read', name: '항목 조회', description: '조회' }
    let assignedPermission = false
    let assignedUser = false
    const member = { id: 3, nickname: '홍길동', emails: ['hong@example.com'] }
    server.use(
      http.get('*/api/auth/me', () =>
        HttpResponse.json({
          code: 'OK',
          data: {
            exp: Math.floor(Date.now() / 1000) + 900,
            user: {
              roles: ['admin'],
              permissions: [
                'rbac.read',
                'rbac.role.manage',
                'rbac.user_role.manage',
                'auth.user.read',
              ],
            },
          },
        }),
      ),
      http.get('*/api/rbac/roles/1', () => HttpResponse.json({ code: 'OK', data: role })),
      http.get('*/api/rbac/permissions', () =>
        HttpResponse.json({ code: 'OK', data: [permission] }),
      ),
      http.get('*/api/rbac/roles/1/permissions', () =>
        HttpResponse.json({ code: 'OK', data: assignedPermission ? [permission] : [] }),
      ),
      http.post('*/api/rbac/roles/1/permissions/2', () => {
        assignedPermission = true
        return HttpResponse.json({ code: 'OK' })
      }),
      http.get('*/api/rbac/roles/1/users', () =>
        HttpResponse.json({
          code: 'OK',
          data: { items: assignedUser ? [member] : [], has_next: false },
        }),
      ),
      http.get('*/api/rbac/roles/1/users/search', () =>
        HttpResponse.json({
          code: 'OK',
          data: { items: [{ ...member, assigned: assignedUser }], has_next: false },
        }),
      ),
      http.post('*/api/rbac/users/3/roles/1', () => {
        assignedUser = true
        return HttpResponse.json({ code: 'OK' })
      }),
    )
    renderWithProviders('/admin/roles/1', 'admin-token')
    await user.click(await screen.findByRole('button', { name: '항목 조회 추가' }))
    expect(await screen.findByRole('button', { name: '항목 조회 해제' })).toBeVisible()
    await user.click(screen.getByRole('button', { name: '사용자 설정' }))
    await user.click(await screen.findByRole('button', { name: '+ 사용자 추가' }))
    await user.click(await screen.findByRole('button', { name: '홍길동 추가' }))
    const dialog = screen.getByRole('dialog', { name: '사용자 추가' })
    expect(await within(dialog).findByRole('button', { name: '홍길동 할당됨' })).toBeDisabled()
    await user.click(within(dialog).getByRole('button', { name: '사용자 추가 닫기' }))
    expect(await screen.findByRole('button', { name: '홍길동 해제' })).toBeVisible()
  })

  it('권한을 추가 가능 여부와 이름순으로 정렬한다', async () => {
    const role = { id: 1, code: 'operator', name: '운영자', description: '' }
    const items = [
      { id: 1, code: 'z', name: '다 권한', description: '' },
      { id: 2, code: 'b', name: '나 권한', description: '' },
      { id: 3, code: 'a', name: '가 권한', description: '' },
      { id: 4, code: 'c', name: '라 권한', description: '' },
    ]
    server.use(
      http.get('*/api/auth/me', () =>
        HttpResponse.json({
          code: 'OK',
          data: {
            exp: Math.floor(Date.now() / 1000) + 900,
            user: { roles: ['admin'], permissions: ['rbac.read', 'rbac.role.manage'] },
          },
        }),
      ),
      http.get('*/api/rbac/roles/1', () => HttpResponse.json({ code: 'OK', data: role })),
      http.get('*/api/rbac/permissions', () => HttpResponse.json({ code: 'OK', data: items })),
      http.get('*/api/rbac/roles/1/permissions', () =>
        HttpResponse.json({ code: 'OK', data: [items[0], items[2]] }),
      ),
    )
    renderWithProviders('/admin/roles/1', 'admin-token')
    await screen.findByRole('button', { name: '가 권한 해제' })
    expect(
      screen
        .getAllByRole('listitem')
        .slice(-4)
        .map((item) => within(item).getByRole('button').getAttribute('aria-label')),
    ).toEqual(['나 권한 추가', '라 권한 추가', '가 권한 해제', '다 권한 해제'])
  })
})
