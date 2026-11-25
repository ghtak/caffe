import { screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { http, HttpResponse } from 'msw'
import { describe, expect, it } from 'vitest'
import { apiClient } from '@/shared/api/client'
import { server } from '../../../../test/msw/server'
import { renderWithProviders } from '../../../../test/renderWithProviders'

const tokens = (token: string) => ({
  data: {
    access_token: token,
    expires_at: Math.floor(Date.now() / 1000) + 900,
    claims: { permissions: [] },
  },
})

describe('contract', () => {
  it('저장된 토큰이 없는 새 탭에서 쿠키 세션을 복구한다', async () => {
    server.use(
      http.post('*/api/auth/web/refresh', async ({ request }) => {
        expect(request.headers.get('x-conbira-request')).toBe('1')
        expect(await request.text()).toBe('')
        return HttpResponse.json(tokens('restored-access'))
      }),
      http.get('*/api/auth/me', () =>
        HttpResponse.json({
          data: { exp: Math.floor(Date.now() / 1000) + 900, user: { permissions: [] } },
        }),
      ),
    )
    renderWithProviders('/')
    expect(await screen.findByRole('button', { name: '로그아웃' })).toBeVisible()
    expect(sessionStorage.getItem('conbira.access-token')).toBe('restored-access')
  })

  it('동시 인증 실패는 한 번 갱신하고 새 토큰으로 재시도한다', async () => {
    let refreshCount = 0
    server.use(
      http.post('*/api/auth/web/refresh', () =>
        HttpResponse.json(tokens(++refreshCount === 1 ? 'old-access' : 'new-access')),
      ),
      http.get('*/api/auth/me', () =>
        HttpResponse.json({
          data: { exp: Math.floor(Date.now() / 1000) + 900, user: { permissions: [] } },
        }),
      ),
      http.get('*/api/protected/:id', ({ request }) =>
        request.headers.get('authorization') === 'Bearer new-access'
          ? HttpResponse.json({ ok: true })
          : new HttpResponse(null, { status: 401 }),
      ),
    )
    renderWithProviders('/')
    await screen.findByRole('button', { name: '로그아웃' })
    const headers = { Authorization: 'Bearer old-access' }
    const results = await Promise.all([
      apiClient.get('/api/protected/1', { headers }).json(),
      apiClient.get('/api/protected/2', { headers }).json(),
    ])
    expect(results).toEqual([{ ok: true }, { ok: true }])
    expect(refreshCount).toBe(2)
  })

  it('갱신 중 로그아웃은 갱신이 끝난 다음 쿠키 세션을 폐기한다', async () => {
    let release!: () => void
    let started = false
    let loggedOut = false
    let refreshCount = 0
    const waiting = new Promise<void>((resolve) => {
      release = resolve
    })
    server.use(
      http.post('*/api/auth/web/refresh', async () => {
        if (++refreshCount === 1) return HttpResponse.json(tokens('old-access'))
        started = true
        await waiting
        return HttpResponse.json(tokens('new-access'))
      }),
      http.get('*/api/auth/me', () =>
        HttpResponse.json({
          data: { exp: Math.floor(Date.now() / 1000) + 900, user: { permissions: [] } },
        }),
      ),
      http.post('*/api/auth/web/logout', () => {
        loggedOut = true
        return HttpResponse.json({ code: 'OK' })
      }),
      http.get('*/api/protected/1', ({ request }) =>
        request.headers.get('authorization') === 'Bearer new-access'
          ? HttpResponse.json({ ok: true })
          : new HttpResponse(null, { status: 401 }),
      ),
    )
    renderWithProviders('/')
    const button = await screen.findByRole('button', { name: '로그아웃' })
    const request = apiClient.get('/api/protected/1', {
      headers: { Authorization: 'Bearer old-access' },
    })
    await waitFor(() => expect(started).toBe(true))
    await userEvent.setup().click(button)
    expect(loggedOut).toBe(false)
    release()
    await request
    expect(await screen.findByRole('link', { name: '로그인' })).toBeVisible()
    expect(loggedOut).toBe(true)
  })

  it('로그아웃은 쿠키 경로로 요청하고 로컬 세션을 지운다', async () => {
    let loggedOut = false
    server.use(
      http.post('*/api/auth/web/refresh', () => HttpResponse.json(tokens('access'))),
      http.get('*/api/auth/me', () =>
        HttpResponse.json({
          data: { exp: Math.floor(Date.now() / 1000) + 900, user: { permissions: [] } },
        }),
      ),
      http.post('*/api/auth/web/logout', async ({ request }) => {
        expect(await request.text()).toBe('')
        loggedOut = true
        return HttpResponse.json({ code: 'OK' })
      }),
    )
    renderWithProviders('/')
    await userEvent.setup().click(await screen.findByRole('button', { name: '로그아웃' }))
    expect(await screen.findByRole('link', { name: '로그인' })).toBeVisible()
    expect(loggedOut).toBe(true)
    expect(sessionStorage.getItem('conbira.access-token')).toBeNull()
  })
})

describe('regression', () => {
  it('로그아웃 실패 후 재시도하면 세션을 종료한다', async () => {
    let requests = 0
    server.use(
      http.post('*/api/auth/web/refresh', () => HttpResponse.json(tokens('access'))),
      http.get('*/api/auth/me', () =>
        HttpResponse.json({
          data: { exp: Math.floor(Date.now() / 1000) + 900, user: { permissions: [] } },
        }),
      ),
      http.post('*/api/auth/web/logout', () =>
        ++requests === 1
          ? new HttpResponse(null, { status: 500 })
          : HttpResponse.json({ code: 'OK' }),
      ),
    )
    renderWithProviders('/')
    const user = userEvent.setup()
    await user.click(await screen.findByRole('button', { name: '로그아웃' }))
    expect(await screen.findByText('로그아웃하지 못했습니다. 다시 시도해 주세요.')).toBeVisible()
    expect(sessionStorage.getItem('conbira.access-token')).toBe('access')
    await user.click(screen.getByRole('button', { name: '다시 시도' }))
    expect(await screen.findByRole('link', { name: '로그인' })).toBeVisible()
    expect(requests).toBe(2)
  })

  it('세션 갱신 후 사용자 목록의 입력 상태를 유지한다', async () => {
    let refreshes = 0
    const permissions = ['auth.user.read']
    server.use(
      http.post('*/api/auth/web/refresh', () =>
        HttpResponse.json({
          data: {
            ...tokens(++refreshes === 1 ? 'old-access' : 'new-access').data,
            claims: { permissions },
          },
        }),
      ),
      http.get('*/api/auth/me', () =>
        HttpResponse.json({
          data: { exp: Math.floor(Date.now() / 1000) + 900, user: { permissions } },
        }),
      ),
      http.get('*/api/auth/users', () =>
        HttpResponse.json({ data: { items: [], page: 1, page_size: 20, has_next: false } }),
      ),
      http.get('*/api/protected/1', ({ request }) =>
        request.headers.get('authorization') === 'Bearer new-access'
          ? HttpResponse.json({ ok: true })
          : new HttpResponse(null, { status: 401 }),
      ),
    )
    renderWithProviders('/admin/users')
    const input = await screen.findByLabelText('이름')
    await userEvent.setup().type(input, '검색 중')
    await apiClient.get('/api/protected/1', { headers: { Authorization: 'Bearer old-access' } })
    expect(screen.getByLabelText('이름')).toHaveValue('검색 중')
  })
})

describe('edge', () => {
  it('초기 세션 복구 장애는 재시도할 수 있다', async () => {
    let attempts = 0
    server.use(
      http.post('*/api/auth/web/refresh', () =>
        ++attempts === 1
          ? new HttpResponse(null, { status: 500 })
          : new HttpResponse(null, { status: 401 }),
      ),
    )
    renderWithProviders('/')
    await userEvent.setup().click(await screen.findByRole('button', { name: '다시 시도' }))
    expect(await screen.findByRole('link', { name: '로그인' })).toBeVisible()
    await waitFor(() => expect(attempts).toBe(2))
  })
})
