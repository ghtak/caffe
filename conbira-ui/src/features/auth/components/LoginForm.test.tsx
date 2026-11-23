import { screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { http, HttpResponse } from 'msw'
import { describe, expect, it } from 'vitest'
import { server } from '../../../../test/msw/server'
import { renderWithProviders } from '../../../../test/renderWithProviders'

describe('contract', () => {
  it('랜딩에서 로그인으로 이동해 자격 증명을 보내고 성공하면 홈으로 돌아간다', async () => {
    const user = userEvent.setup()
    const credentials = { email: 'member@example.test', password: 'example-password' }
    let submitted: unknown
    server.use(
      http.post('*/api/auth/login/local', async ({ request }) => {
        submitted = await request.json()
        return HttpResponse.json({ code: 'OK', data: { access_token: 'member-token' } })
      }),
      http.get('*/api/auth/me', ({ request }) => {
        if (request.headers.get('Authorization') !== 'Bearer member-token') {
          return HttpResponse.json({ code: 'UNAUTHORIZED' }, { status: 401 })
        }
        return HttpResponse.json({
          code: 'OK',
          data: { exp: Math.floor(Date.now() / 1000) + 900, user: { roles: ['user'], permissions: ['rbac.read'] } },
        })
      }),
    )
    const { router } = renderWithProviders('/')
    await screen.findByText('서비스 콘텐츠 영역')

    await user.click(screen.getByRole('link', { name: '로그인' }))
    await user.type(await screen.findByLabelText('이메일'), credentials.email)
    await user.type(screen.getByLabelText('비밀번호'), credentials.password)
    await user.click(screen.getByRole('button', { name: '로그인' }))

    // 입력한 자격 증명을 전송하고 인증된 홈에 로그아웃 메뉴를 표시한다.
    expect(await screen.findByRole('heading', { name: '홈' })).toBeVisible()
    expect(screen.getByRole('button', { name: '로그아웃' })).toBeVisible()
    expect(router.state.location.pathname).toBe('/')
    expect(submitted).toEqual(credentials)
  })
})

describe('edge', () => {
  it('빈 로그인 폼을 제출하면 두 필드의 입력 오류를 표시한다', async () => {
    const user = userEvent.setup()
    renderWithProviders('/login')

    await user.click(await screen.findByRole('button', { name: '로그인' }))

    // 이메일과 비밀번호에 입력 오류가 표시된다.
    expect(await screen.findByText('이메일 주소를 입력해 주세요.')).toBeVisible()
    expect(screen.getByText('비밀번호를 입력해 주세요.')).toBeVisible()
  })

  it('인증이 거부되면 로그인 화면에 오류와 재시도 가능한 제출 버튼을 표시한다', async () => {
    const user = userEvent.setup()
    server.use(http.post('*/api/auth/login/local', () =>
      HttpResponse.json({ code: 'UNAUTHORIZED' }, { status: 401 })))
    const { router } = renderWithProviders('/login')

    await user.type(await screen.findByLabelText('이메일'), 'member@example.test')
    await user.type(screen.getByLabelText('비밀번호'), 'incorrect-password')
    await user.click(screen.getByRole('button', { name: '로그인' }))

    // 서버가 인증을 거부하면 화면에 남아 다시 제출할 수 있다.
    expect(await screen.findByRole('alert')).toHaveTextContent('이메일 또는 비밀번호를 확인해 주세요.')
    expect(router.state.location.pathname).toBe('/login')
    expect(screen.getByRole('button', { name: '로그인' })).toBeEnabled()
  })
})

describe('regression', () => {
  it.each(['로그인 응답', '세션 확인 응답'])('%s을 기다리다 취소하면 로그인 상태가 저장되지 않는다', async (stage) => {
    const user = userEvent.setup()
    const token = 'cancelled-token'
    let releaseResponse!: () => void
    const responseGate = new Promise<void>((resolve) => { releaseResponse = resolve })
    let delayedRequest: Request | undefined

    async function delayResponse(request: Request) {
      delayedRequest = request
      await responseGate
    }

    server.use(
      http.post('*/api/auth/login/local', async ({ request }) => {
        if (stage === '로그인 응답') await delayResponse(request)
        return HttpResponse.json({ data: { access_token: token } })
      }),
      http.get('*/api/auth/me', async ({ request }) => {
        if (stage === '세션 확인 응답') await delayResponse(request)
        return HttpResponse.json({
          data: { exp: Math.floor(Date.now() / 1000) + 900, user: { roles: ['user'] } },
        })
      }),
    )
    renderWithProviders('/login')

    try {
      await user.type(await screen.findByLabelText('이메일'), 'member@example.test')
      await user.type(screen.getByLabelText('비밀번호'), 'example-password')
      await user.click(screen.getByRole('button', { name: '로그인' }))
      await waitFor(() => expect(delayedRequest).toBeDefined())
      await user.click(screen.getByRole('link', { name: '취소' }))

      // 응답을 보류한 동안 취소하면 요청을 중단하고 공개 홈으로 돌아간다.
      await waitFor(() => expect(delayedRequest?.signal.aborted).toBe(true))
      releaseResponse()
      expect(await screen.findByRole('heading', { name: '홈' })).toBeVisible()
      expect(screen.getByRole('link', { name: '로그인' })).toBeVisible()
      expect(screen.queryByRole('button', { name: '로그아웃' })).not.toBeInTheDocument()
      expect(sessionStorage.getItem('conbira.access-token')).toBeNull()
    } finally {
      releaseResponse()
    }
  })
})
