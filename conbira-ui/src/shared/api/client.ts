import ky from 'ky'

type RefreshHandler = (accessToken: string) => Promise<string | null>
let refreshHandler: RefreshHandler | null = null

export function setRefreshHandler(handler: RefreshHandler | null) {
  refreshHandler = handler
}

export const apiClient = ky.create({
  baseUrl: globalThis.location.origin,
  hooks: {
    afterResponse: [
      async ({ request, response }) => {
        const authorization = request.headers.get('Authorization')
        if (response.status !== 401 || !authorization?.startsWith('Bearer ') || !refreshHandler)
          return
        const token = await refreshHandler(authorization.slice(7))
        if (!token) return
        const retry = new Request(request)
        retry.headers.set('Authorization', `Bearer ${token}`)
        return fetch(retry)
      },
    ],
  },
})
