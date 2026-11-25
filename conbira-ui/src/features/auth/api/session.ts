import { queryOptions } from '@tanstack/react-query'
import { apiClient } from '@/shared/api/client'
import { z } from 'zod'
import { loginResponse, sessionResponse, type Credentials } from '../types/auth.types'

export const authKeys = {
  all: ['auth'] as const,
  session: () => [...authKeys.all, 'session'] as const,
}

export async function startGoogleLogin(signal: AbortSignal) {
  const body = await apiClient.post('/api/auth/login/google', { signal }).json()
  return z
    .object({
      data: z.object({
        authorization_url: z.url(),
        state: z.string().min(1),
        browser_proof: z.string().min(1),
      }),
    })
    .parse(body).data
}

export async function login(credentials: Credentials, signal: AbortSignal) {
  const body = await apiClient.post('/api/auth/login/local', { json: credentials, signal }).json()
  return resolveLogin(body, signal)
}

export async function completeGoogleLogin(
  input: { code: string; state: string; browser_proof: string },
  signal: AbortSignal,
) {
  const body = await apiClient
    .post('/api/auth/login/google/complete', { json: input, signal })
    .json()
  return resolveLogin(body, signal)
}

async function resolveLogin(body: unknown, signal: AbortSignal) {
  const accessToken = loginResponse.parse(body).data.access_token
  signal.throwIfAborted()
  const claims = await fetchSession(accessToken, signal)
  return { accessToken, claims }
}

async function fetchSession(token: string, signal: AbortSignal) {
  const body = await apiClient
    .get('/api/auth/me', {
      headers: { Authorization: `Bearer ${token}` },
      signal,
    })
    .json()
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
