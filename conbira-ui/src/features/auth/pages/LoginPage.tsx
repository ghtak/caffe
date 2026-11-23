import { Link } from '@tanstack/react-router'
import { useLayoutEffect, useRef } from 'react'
import { Button } from '@/shared/components/ui/Button'
import { LoginForm } from '../components/LoginForm'
import type { Credentials } from '../types/auth.types'

export function LoginPage({ onLogin }: {
  onLogin: (credentials: Credentials, signal: AbortSignal) => Promise<void>
}) {
  const loginScope = useRef<AbortController | null>(null)

  useLayoutEffect(() => {
    const scope = new AbortController()
    loginScope.current = scope
    return () => {
      scope.abort()
      loginScope.current = null
    }
  }, [])

  async function submit(credentials: Credentials) {
    const signal = loginScope.current?.signal
    if (!signal || signal.aborted) return
    try {
      await onLogin(credentials, signal)
    } catch (error) {
      if (!signal.aborted) throw error
    }
  }

  return (
    <main className="grid min-h-90 grid-cols-[minmax(0,var(--container-sm))] place-content-center p-4">
      <LoginForm onLogin={submit} cancel={
        <Button asChild variant="outline">
          <Link to="/" onClick={(event) => {
            if (!event.defaultPrevented && event.button === 0 &&
                !event.altKey && !event.ctrlKey && !event.metaKey && !event.shiftKey) {
              loginScope.current?.abort()
            }
          }}>취소</Link>
        </Button>
      } />
    </main>
  )
}
