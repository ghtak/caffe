import { WEB_REQUEST_HEADERS } from '../constants'
import { queryOptions } from '@tanstack/react-query'
import { apiClient } from '@/shared/api/client'
import {
  googleLoginStartResponse,
  loginResponse,
  refreshResponse,
  sessionResponse,
  type Credentials,
} from '../types/auth.types'

export const authKeys = {
  all: ['auth'] as const,
  session: (token: string | null) => [...authKeys.all, 'session', token] as const,
}

export async function startGoogleLogin(signal: AbortSignal) {
  const body = await apiClient.post('/api/auth/login/google', { signal }).json()
  return googleLoginStartResponse.parse(body).data
}

export async function login(credentials: Credentials, signal: AbortSignal) {
  const body = await apiClient
    .post('/api/auth/web/login/local', { json: credentials, signal, headers: WEB_REQUEST_HEADERS })
    .json()
  return resolveLogin(body, signal)
}

export async function completeGoogleLogin(
  input: { code: string; state: string; browser_proof: string },
  signal: AbortSignal,
) {
  const body = await apiClient
    .post('/api/auth/web/login/google/complete', {
      json: input,
      signal,
      headers: WEB_REQUEST_HEADERS,
    })
    .json()
  return resolveLogin(body, signal)
}

async function resolveLogin(body: unknown, signal: AbortSignal) {
  const { access_token: accessToken } = loginResponse.parse(body).data
  signal.throwIfAborted()
  const claims = await fetchSession(accessToken, signal)
  return { accessToken, claims }
}

export async function refreshSession(signal: AbortSignal) {
  const body = await apiClient
    .post('/api/auth/web/refresh', { signal, retry: 0, headers: WEB_REQUEST_HEADERS })
    .json()
  return refreshResponse.parse(body).data
}

export async function logoutSession() {
  await apiClient.post('/api/auth/web/logout', { retry: 0, headers: WEB_REQUEST_HEADERS })
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
    meta: { requiresSession: true },
    queryKey: authKeys.session(token),
    enabled: token !== null,
    staleTime: 0,
    retry: false,
    queryFn: async ({ signal }) => {
      if (!token) throw new Error('Missing access token')
      return fetchSession(token, signal)
    },
  })
}
