import { screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { http, HttpResponse } from 'msw'
import { describe, expect, it } from 'vitest'
import { apiClient } from '@/shared/api/client'
import { server } from '../../../../test/msw/server'
import { renderWithProviders } from '../../../../test/renderWithProviders'

describe('contract', () => {
  it('동시 인증 실패 시 한 번 갱신하고 각 요청을 새 토큰으로 재시도한다', async () => {
    let refreshCount = 0
    const tokens: string[] = []
    sessionStorage.setItem('conbira.refresh-token', 'old-refresh')
    server.use(
      http.get('*/api/auth/me', () =>
        HttpResponse.json({
          data: { exp: Math.floor(Date.now() / 1000) + 900, user: { permissions: [] } },
        }),
      ),
      http.post('*/api/auth/refresh', async ({ request }) => {
        refreshCount++
        expect(await request.json()).toEqual({ refresh_token: 'old-refresh' })
        return HttpResponse.json({
          data: {
            access_token: 'new-access',
            refresh_token: 'new-refresh',
            expires_at: Math.floor(Date.now() / 1000) + 900,
            claims: { roles: [], permissions: [] },
          },
        })
      }),
      http.get('*/api/protected/:id', ({ request }) => {
        const authorization = request.headers.get('Authorization') ?? ''
        tokens.push(authorization)
        return authorization === 'Bearer new-access'
          ? HttpResponse.json({ ok: true })
          : new HttpResponse(null, { status: 401 })
      }),
    )
    renderWithProviders('/', 'old-access')
    await screen.findByRole('button', { name: '로그아웃' })
    const headers = { Authorization: 'Bearer old-access' }
    const results = await Promise.all([
      apiClient.get('/api/protected/1', { headers }).json(),
      apiClient.get('/api/protected/2', { headers }).json(),
    ])
    // 요청 두 개가 하나의 회전을 공유하고 각각 성공한다.
    expect(results).toEqual([{ ok: true }, { ok: true }])
    expect(refreshCount).toBe(1)
    expect(tokens.filter((token) => token === 'Bearer new-access')).toHaveLength(2)
    expect(sessionStorage.getItem('conbira.refresh-token')).toBe('new-refresh')
  })
})

describe('regression', () => {
  it('로그아웃 실패 후 다시 시도하면 로그아웃 요청을 다시 보낸다', async () => {
    let requests = 0
    sessionStorage.setItem('conbira.refresh-token', 'refresh-token')
    server.use(
      http.get('*/api/auth/me', () =>
        HttpResponse.json({
          data: { exp: Math.floor(Date.now() / 1000) + 900, user: { roles: [], permissions: [] } },
        }),
      ),
      http.post('*/api/auth/logout', ({ request }) => {
        expect(request.headers.get('Authorization')).toBe('Bearer access-token')
        requests++
        return requests === 1
          ? new HttpResponse(null, { status: 500 })
          : HttpResponse.json({ code: 'OK' })
      }),
    )
    renderWithProviders('/', 'access-token')
    const user = userEvent.setup()
    await user.click(await screen.findByRole('button', { name: '로그아웃' }))
    expect(await screen.findByText('로그아웃하지 못했습니다. 다시 시도해 주세요.')).toBeVisible()
    expect(sessionStorage.getItem('conbira.access-token')).toBe('access-token')
    await user.click(screen.getByRole('button', { name: '다시 시도' }))
    // 두 번째 로그아웃이 성공하면 토큰을 지우고 비로그인 상태가 된다.
    expect(await screen.findByRole('link', { name: '로그인' })).toBeVisible()
    expect(requests).toBe(2)
    expect(sessionStorage.getItem('conbira.access-token')).toBeNull()
    expect(sessionStorage.getItem('conbira.refresh-token')).toBeNull()
  })

  it('세션 갱신 후 사용자 목록의 입력 상태를 유지한다', async () => {
    const permissions = ['auth.user.read']
    let releaseMe!: () => void
    let newMeStarted = false
    const waitForMe = new Promise<void>((resolve) => {
      releaseMe = resolve
    })
    sessionStorage.setItem('conbira.refresh-token', 'old-refresh')
    server.use(
      http.get('*/api/auth/me', async ({ request }) => {
        if (request.headers.get('Authorization') === 'Bearer new-access') {
          newMeStarted = true
          await waitForMe
        }
        return HttpResponse.json({
          data: {
            exp: Math.floor(Date.now() / 1000) + 900,
            user: { roles: ['admin'], permissions },
          },
        })
      }),
      http.get('*/api/auth/users', () =>
        HttpResponse.json({ data: { items: [], page: 1, page_size: 20, has_next: false } }),
      ),
      http.post('*/api/auth/refresh', () =>
        HttpResponse.json({
          data: {
            access_token: 'new-access',
            refresh_token: 'new-refresh',
            expires_at: Math.floor(Date.now() / 1000) + 900,
            claims: { roles: ['admin'], permissions },
          },
        }),
      ),
      http.get('*/api/protected/1', ({ request }) =>
        request.headers.get('Authorization') === 'Bearer new-access'
          ? HttpResponse.json({ ok: true })
          : new HttpResponse(null, { status: 401 }),
      ),
    )
    renderWithProviders('/admin/users', 'old-access')
    const input = await screen.findByLabelText('이름')
    await userEvent.setup().type(input, '검색 중')
    try {
      await apiClient.get('/api/protected/1', { headers: { Authorization: 'Bearer old-access' } })
      await waitFor(() => expect(newMeStarted).toBe(true))
      // 새 /me 응답이 지연되어도 기존 입력이 유지된다.
      expect(screen.getByLabelText('이름')).toHaveValue('검색 중')
    } finally {
      releaseMe()
    }
  })
})

describe('edge', () => {
  it('refresh가 거부되면 요청을 반복하지 않고 로그인 화면을 표시한다', async () => {
    let refreshCount = 0
    sessionStorage.setItem('conbira.refresh-token', 'expired-refresh')
    server.use(
      http.get('*/api/auth/me', () => new HttpResponse(null, { status: 401 })),
      http.post('*/api/auth/refresh', () => {
        refreshCount++
        return new HttpResponse(null, { status: 401 })
      }),
    )
    renderWithProviders('/', 'expired-access')
    // 갱신 실패는 세션을 지우고 재요청을 반복하지 않는다.
    expect(await screen.findByRole('link', { name: '로그인' })).toBeVisible()
    expect(refreshCount).toBe(1)
    expect(sessionStorage.getItem('conbira.refresh-token')).toBeNull()
  })
})
