import { useQuery, useQueryClient } from '@tanstack/react-query'
import { isHTTPError } from 'ky'
import { createContext, useCallback, useContext, useEffect, useState, type ReactNode } from 'react'
import { authKeys, login, sessionOptions, type Credentials } from '@/features/auth'
import { rbacKeys } from '@/features/rbac'

export type Session =
  | { status: 'guest' | 'loading' | 'error' }
  | { status: 'authenticated'; token: string; canManage: boolean }

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
    signal.throwIfAborted()
    const { accessToken, claims } = await login(credentials, signal)
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
      canManage: claims.data.user.roles.includes('admin'),
    }
  } else {
    session = { status: 'loading' }
  }

  return (
    <SessionContext.Provider value={{ session, signIn, signOut, retry: () => void claims.refetch() }}>
      {children}
    </SessionContext.Provider>
  )
}

export function useSession() {
  const context = useContext(SessionContext)
  if (!context) throw new Error('SessionProvider is required')
  return context
}
