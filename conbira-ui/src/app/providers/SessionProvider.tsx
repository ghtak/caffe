import { useQuery, useQueryClient } from '@tanstack/react-query'
import { isHTTPError } from 'ky'
import { createContext, useCallback, useContext, useEffect, useState, type ReactNode } from 'react'
import {
  authKeys,
  completeGoogleLogin,
  login,
  sessionOptions,
  type Credentials,
} from '@/features/auth'
import { rbacKeys } from '@/features/rbac'
import { userKeys } from '@/features/users'

export type Session =
  | { status: 'guest' | 'loading' | 'error' }
  | { status: 'authenticated'; token: string; permissions: string[] }

const storageKey = 'conbira.access-token'

function readToken() {
  try {
    return sessionStorage.getItem(storageKey)
  } catch {
    return null
  }
}

function persistToken(token: string | null) {
  try {
    if (token === null) sessionStorage.removeItem(storageKey)
    else sessionStorage.setItem(storageKey, token)
  } catch {
    // Keep session changes in memory when browser storage is unavailable.
  }
}

const SessionContext = createContext<{
  session: Session
  signIn: (credentials: Credentials, signal: AbortSignal) => Promise<void>
  signInGoogle: (
    input: { code: string; state: string; browser_proof: string },
    signal: AbortSignal,
  ) => Promise<void>
  signOut: () => void
  retry: () => void
} | null>(null)

export function SessionProvider({ children }: { children: ReactNode }) {
  const queryClient = useQueryClient()
  const [token, setToken] = useState(readToken)
  const claims = useQuery(sessionOptions(token))

  const signOut = useCallback(() => {
    persistToken(null)
    setToken(null)
    queryClient.removeQueries({ queryKey: authKeys.all })
    queryClient.removeQueries({ queryKey: rbacKeys.all })
    queryClient.removeQueries({ queryKey: userKeys.all })
  }, [queryClient])

  const exp = claims.data?.exp
  useEffect(() => {
    if (!token || exp === undefined) return
    const timeout = window.setTimeout(signOut, Math.max(0, exp * 1000 - Date.now()))
    return () => window.clearTimeout(timeout)
  }, [token, exp, signOut])

  const expired = isHTTPError(claims.error) && claims.error.response.status === 401
  useEffect(() => {
    if (!token || !expired) return
    const timeout = window.setTimeout(signOut, 0)
    return () => window.clearTimeout(timeout)
  }, [token, expired, signOut])

  async function signIn(credentials: Credentials, signal: AbortSignal) {
    saveLogin(await login(credentials, signal), signal)
  }

  async function signInGoogle(
    input: { code: string; state: string; browser_proof: string },
    signal: AbortSignal,
  ) {
    saveLogin(await completeGoogleLogin(input, signal), signal)
  }

  function saveLogin(
    { accessToken, claims }: Awaited<ReturnType<typeof login>>,
    signal: AbortSignal,
  ) {
    signal.throwIfAborted()
    persistToken(accessToken)
    queryClient.setQueryData(authKeys.session(), claims)
    setToken(accessToken)
  }

  let session: Session
  if (!token || expired) {
    session = { status: 'guest' }
  } else if (claims.isError) {
    session = { status: 'error' }
  } else if (claims.data) {
    session = {
      status: 'authenticated',
      token,
      permissions: claims.data.user.permissions,
    }
  } else {
    session = { status: 'loading' }
  }

  return (
    <SessionContext.Provider
      value={{ session, signIn, signInGoogle, signOut, retry: () => void claims.refetch() }}
    >
      {children}
    </SessionContext.Provider>
  )
}

export function useSession() {
  const context = useContext(SessionContext)
  if (!context) throw new Error('SessionProvider is required')
  return context
}
