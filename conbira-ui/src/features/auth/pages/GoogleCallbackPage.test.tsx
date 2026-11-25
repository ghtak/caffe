import { screen, waitFor } from '@testing-library/react'
import { http, HttpResponse } from 'msw'
import { describe, expect, it } from 'vitest'
import { server } from '../../../../test/msw/server'
import { renderWithProviders } from '../../../../test/renderWithProviders'

describe('contract', () => {
  it('일치하는 콜백을 한 번 완료하고 기존 세션으로 홈에 로그인한다', async () => {
    sessionStorage.setItem(
      'conbira.google-login',
      JSON.stringify({ state: 'tab-state', browser_proof: 'proof' }),
    )
    let submitted: unknown
    let requests = 0
    server.use(
      http.post('*/api/auth/login/google/complete', async ({ request }) => {
        requests++
        submitted = await request.json()
        return HttpResponse.json({
          data: { access_token: 'google-token', refresh_token: 'refresh-token' },
        })
      }),
      http.get('*/api/auth/me', () =>
        HttpResponse.json({
          data: { exp: Math.floor(Date.now() / 1000) + 900, user: { permissions: [] } },
        }),
      ),
    )
    renderWithProviders('/login/google/callback?code=auth-code&state=tab-state')

    // 저장된 증명으로 한 번만 완료하고 인증된 홈을 표시한다.
    expect(await screen.findByRole('button', { name: '로그아웃' })).toBeVisible()
    expect(submitted).toEqual({ code: 'auth-code', state: 'tab-state', browser_proof: 'proof' })
    expect(requests).toBe(1)
  })
})

describe('edge', () => {
  it.each([
    ['/login/google/callback?code=auth-code&state=other', false],
    ['/login/google/callback?error=access_denied&state=tab-state', true],
  ])('유효하지 않은 콜백 %s는 완료 요청 없이 오류를 표시한다', async (path, consumed) => {
    sessionStorage.setItem(
      'conbira.google-login',
      JSON.stringify({ state: 'tab-state', browser_proof: 'proof' }),
    )
    let requests = 0
    server.use(
      http.post('*/api/auth/login/google/complete', () => {
        requests++
        return HttpResponse.json({})
      }),
    )
    const { router } = renderWithProviders(path)

    // 실패 시 자격증명이 URL에 남지 않고 완료 요청을 보내지 않는다.
    expect(await screen.findByRole('alert')).toHaveTextContent('Google 로그인에 실패했습니다')
    await waitFor(() => expect(router.state.location.href).toBe('/login/google/callback'))
    expect(requests).toBe(0)
    expect(sessionStorage.getItem('conbira.google-login') === null).toBe(consumed)
  })
})
