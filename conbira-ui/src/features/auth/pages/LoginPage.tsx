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
    <main className="relative grid min-h-[calc(100vh-4.5rem)] grid-cols-[minmax(0,28rem)] place-content-center overflow-hidden px-4 py-12 sm:py-16">
      <div
        aria-hidden="true"
        className="pointer-events-none absolute -top-32 -left-32 size-80 rounded-full bg-accent/50"
      />
      <div
        aria-hidden="true"
        className="pointer-events-none absolute -right-32 -bottom-40 size-96 rotate-45 rounded-3xl bg-accent/40"
      />
      <div className="relative z-10">
        <LoginForm
          onLogin={submit}
          loginSignal={() => loginScope.current?.signal}
          cancel={
            <Button asChild variant="link">
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
