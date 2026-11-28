import { Link } from '@tanstack/react-router'
import { useLayoutEffect, useRef } from 'react'
import { Button } from '@/shared/components/ui/Button'
import { LoginForm } from '../components/LoginForm'
import type { Credentials } from '../types/auth.types'

export function LoginPage({
  onLogin,
}: {
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
    <main className="flex flex-1 items-center justify-center px-4 py-12">
      <div className="w-full max-w-md">
        <LoginForm
          onLogin={submit}
          loginSignal={() => loginScope.current?.signal}
          cancel={
            <Button asChild variant="link" size="sm">
              <Link
                to="/"
                onClick={(event) => {
                  if (
                    !event.defaultPrevented &&
                    event.button === 0 &&
                    !event.altKey &&
                    !event.ctrlKey &&
                    !event.metaKey &&
                    !event.shiftKey
                  ) {
                    loginScope.current?.abort()
                  }
                }}
              >
                취소
              </Link>
            </Button>
          }
        />
      </div>
    </main>
  )
}
