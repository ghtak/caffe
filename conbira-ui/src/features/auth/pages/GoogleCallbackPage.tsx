import { Link, Navigate } from '@tanstack/react-router'
import { useEffect, useState } from 'react'
import { z } from 'zod'
import { Alert, AlertDescription } from '@/shared/components/ui/Alert'
import { Button } from '@/shared/components/ui/Button'
import { googleLoginStorageKey } from '../types/auth.types'

export function GoogleCallbackPage({
  search,
  signIn,
  clearUrl,
}: {
  search: { code?: string; state?: string; error?: string }
  signIn: (
    input: { code: string; state: string; browser_proof: string },
    signal: AbortSignal,
  ) => Promise<void>
  clearUrl: () => void
}) {
  const [initial] = useState(() => ({ search, signIn, clearUrl }))
  const [status, setStatus] = useState<'pending' | 'error' | 'success'>('pending')

  useEffect(() => {
    const controller = new AbortController()
    queueMicrotask(async () => {
      if (controller.signal.aborted) return
      try {
        initial.clearUrl()
        const stored = z
          .object({ state: z.string().min(1), browser_proof: z.string().min(1) })
          .parse(JSON.parse(sessionStorage.getItem(googleLoginStorageKey) ?? 'null'))
        if (!initial.search.state || initial.search.state !== stored.state)
          throw new Error('Invalid state')
        sessionStorage.removeItem(googleLoginStorageKey)
        if (initial.search.error || !initial.search.code) throw new Error('Google login failed')
        await initial.signIn(
          { code: initial.search.code, state: stored.state, browser_proof: stored.browser_proof },
          controller.signal,
        )
        if (!controller.signal.aborted) setStatus('success')
      } catch {
        if (!controller.signal.aborted) setStatus('error')
      }
    })
    return () => controller.abort()
  }, [initial])

  if (status === 'success') return <Navigate to="/" replace />
  return (
    <main className="grid min-h-90 grid-cols-[minmax(0,var(--container-sm))] place-content-center gap-4 p-4">
      {status === 'pending' ? (
        <p role="status">Google 로그인 중…</p>
      ) : (
        <>
          <Alert variant="destructive">
            <AlertDescription>Google 로그인에 실패했습니다. 다시 시도해 주세요.</AlertDescription>
          </Alert>
          <Button asChild variant="outline">
            <Link to="/login">로그인으로 돌아가기</Link>
          </Button>
        </>
      )}
    </main>
  )
}
