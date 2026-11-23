import { queryOptions } from '@tanstack/react-query'
import { apiClient } from '@/shared/api/client'
import { loginResponse, sessionResponse, type Credentials } from '../types/auth.types'

export const authKeys = {
  all: ['auth'] as const,
  session: () => [...authKeys.all, 'session'] as const,
}

export async function login(credentials: Credentials, signal: AbortSignal) {
  const body = await apiClient.post('/api/auth/login/local', { json: credentials, signal }).json()
  const accessToken = loginResponse.parse(body).data.access_token
  signal.throwIfAborted()
  const claims = await fetchSession(accessToken, signal)
  return { accessToken, claims }
}

async function fetchSession(token: string, signal: AbortSignal) {
  const body = await apiClient.get('/api/auth/me', {
    headers: { Authorization: `Bearer ${token}` },
    signal,
  }).json()
  return sessionResponse.parse(body).data
}

export function sessionOptions(token: string | null) {
  return queryOptions({
    queryKey: authKeys.session(),
    enabled: token !== null,
    staleTime: 0,
    retry: false,
    queryFn: async ({ signal }) => {
      if (!token) throw new Error('Missing access token')
      return fetchSession(token, signal)
    },
  })
}
