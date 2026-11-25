import { http, HttpResponse } from 'msw'
import { setupServer } from 'msw/node'

export const server = setupServer(
  http.post('*/api/auth/web/refresh', async ({ request }) => {
    const token =
      request.headers.get('cookie')?.match(/(?:^|; )test-session=([^;]+)/)?.[1] ??
      (typeof window === 'undefined' ? null : sessionStorage.getItem('conbira.access-token'))
    if (!token) return new HttpResponse(null, { status: 401 })
    const response = await fetch(new URL('/api/auth/me', request.url), {
      headers: { Authorization: `Bearer ${token}` },
    })
    if (!response.ok) return new HttpResponse(null, { status: response.status })
    const body = (await response.json()) as {
      data: { exp: number; user: { permissions: string[] } }
    }
    return HttpResponse.json({
      data: { access_token: token, expires_at: body.data.exp, claims: body.data.user },
    })
  }),
)
