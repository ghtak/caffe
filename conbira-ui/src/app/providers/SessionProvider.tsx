import { useQuery, useQueryClient } from '@tanstack/react-query'
import { isHTTPError } from 'ky'
import {
  createContext,
  useCallback,
  useContext,
  useLayoutEffect,
  useRef,
  useState,
  type ReactNode,
} from 'react'
import {
  authKeys,
  completeGoogleLogin,
  login,
  logoutSession,
  refreshSession,
  sessionOptions,
  type Credentials,
} from '@/features/auth'
import { rbacKeys } from '@/features/rbac'
import { userKeys } from '@/features/users'
import { setRefreshHandler } from '@/shared/api/client'

export type Session =
  | { status: 'guest' | 'loading' }
  | { status: 'error'; reason: 'logout' | 'session' }
  | { status: 'authenticated'; token: string; permissions: string[] }

const storageKey = 'conbira.access-token'
const refreshKey = 'conbira.refresh-token'

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
  signOut: () => Promise<void>
  retry: () => void
} | null>(null)

export function SessionProvider({ children }: { children: ReactNode }) {
  const queryClient = useQueryClient()
  const [token, setToken] = useState(readToken)
  const accessToken = useRef(token)
  const previousToken = useRef<string | null>(null)
  const [refreshError, setRefreshError] = useState(false)
  const [logoutError, setLogoutError] = useState(false)
  const refreshToken = useRef<string | null>(null)
  const pending = useRef<Promise<string | null> | null>(null)
  const signingOut = useRef(false)
  const generation = useRef(0)
  if (refreshToken.current === null) {
    try {
      refreshToken.current = sessionStorage.getItem(refreshKey)
    } catch {
      /* Storage unavailable. */
    }
  }
  const claims = useQuery(sessionOptions(token))

  const clearSession = useCallback(() => {
    generation.current += 1
    setRefreshError(false)
    setLogoutError(false)
    refreshToken.current = null
    try {
      sessionStorage.removeItem(refreshKey)
    } catch {
      /* Storage unavailable. */
    }
    persistToken(null)
    accessToken.current = null
    previousToken.current = null
    setToken(null)
    queryClient.removeQueries({ queryKey: authKeys.all })
    queryClient.removeQueries({ queryKey: rbacKeys.all })
    queryClient.removeQueries({ queryKey: userKeys.all })
  }, [queryClient])

  const signOut = useCallback(async () => {
    if (signingOut.current) return
    signingOut.current = true
    try {
      await pending.current?.catch(() => null)
      const current = refreshToken.current
      if (current) await logoutSession(current)
      clearSession()
    } catch {
      setLogoutError(true)
    } finally {
      signingOut.current = false
    }
  }, [clearSession])

  const refresh = useCallback(
    (failedToken: string): Promise<string | null> => {
      if (signingOut.current) return Promise.resolve(null)
      if (!accessToken.current) return Promise.resolve(null)
      if (failedToken !== accessToken.current)
        return Promise.resolve(failedToken === previousToken.current ? accessToken.current : null)
      if (pending.current) return pending.current
      const current = refreshToken.current
      if (!current) {
        clearSession()
        return Promise.resolve(null)
      }
      const version = generation.current
      setRefreshError(false)
      const task = refreshSession(current, new AbortController().signal)
        .then((tokens) => {
          if (version !== generation.current) return null
          refreshToken.current = tokens.refresh_token
          try {
            sessionStorage.setItem(refreshKey, tokens.refresh_token)
          } catch {
            /* Storage unavailable. */
          }
          persistToken(tokens.access_token)
          previousToken.current = failedToken
          accessToken.current = tokens.access_token
          queryClient.setQueryData(authKeys.session(tokens.access_token), {
            exp: tokens.expires_at,
            user: tokens.claims,
          })
          setToken(tokens.access_token)
          return tokens.access_token
        })
        .catch((error: unknown) => {
          if (version === generation.current && isHTTPError(error) && error.response.status === 401)
            clearSession()
          else if (version === generation.current) setRefreshError(true)
          throw error
        })
        .finally(() => {
          pending.current = null
        })
      pending.current = task
      return task
    },
    [clearSession, queryClient],
  )

  useLayoutEffect(() => {
    setRefreshHandler(refresh)
    return () => setRefreshHandler(null)
  }, [refresh])

  const expired = isHTTPError(claims.error) && claims.error.response.status === 401

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
    { accessToken, refreshToken: newRefreshToken, claims }: Awaited<ReturnType<typeof login>>,
    signal: AbortSignal,
  ) {
    signal.throwIfAborted()
    generation.current += 1
    previousToken.current = null
    refreshToken.current = newRefreshToken
    try {
      sessionStorage.setItem(refreshKey, newRefreshToken)
    } catch {
      /* Storage unavailable. */
    }
    persistToken(accessToken)
    accessTokenRefUpdate(accessToken)
    queryClient.setQueryData(authKeys.session(accessToken), claims)
    setToken(accessToken)
  }

  function accessTokenRefUpdate(value: string) {
    accessToken.current = value
  }

  let session: Session
  if (!token) {
    session = { status: 'guest' }
  } else if (logoutError) {
    session = { status: 'error', reason: 'logout' }
  } else if (refreshError || expired || claims.isError) {
    session = { status: 'error', reason: 'session' }
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
      value={{
        session,
        signIn,
        signInGoogle,
        signOut,
        retry: () => {
          if (logoutError) void signOut()
          else {
            setRefreshError(false)
            void claims.refetch()
          }
        },
      }}
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
