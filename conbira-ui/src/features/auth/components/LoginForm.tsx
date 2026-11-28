import { zodResolver } from '@hookform/resolvers/zod'
import { isHTTPError } from 'ky'
import { useEffect, useRef, useState, type ReactNode } from 'react'
import { useForm } from 'react-hook-form'
import { Alert, AlertDescription } from '@/shared/components/ui/Alert'
import { Button } from '@/shared/components/ui/Button'
import { Card, CardContent, CardFooter, CardHeader } from '@/shared/components/ui/Card'
import { Input } from '@/shared/components/ui/Input'
import { startGoogleLogin } from '../api/session'
import { credentialsSchema, googleLoginStorageKey, type Credentials } from '../types/auth.types'

export function LoginForm({
  onLogin,
  loginSignal,
  cancel,
}: {
  onLogin: (credentials: Credentials) => Promise<void>
  loginSignal: () => AbortSignal | undefined
  cancel: ReactNode
}) {
  const {
    register,
    handleSubmit,
    setError,
    formState: { errors, isSubmitting },
  } = useForm<Credentials>({ resolver: zodResolver(credentialsSchema) })
  const [googlePending, setGooglePending] = useState(false)
  const googleRequest = useRef(false)

  useEffect(() => {
    function restore(event: PageTransitionEvent) {
      if (event.persisted) {
        googleRequest.current = false
        setGooglePending(false)
      }
    }
    window.addEventListener('pageshow', restore)
    return () => window.removeEventListener('pageshow', restore)
  }, [])

  async function startGoogle() {
    if (googleRequest.current) return
    const signal = loginSignal()
    if (!signal || signal.aborted) return
    googleRequest.current = true
    setGooglePending(true)
    try {
      const { authorization_url, state, browser_proof } = await startGoogleLogin(signal)
      signal.throwIfAborted()
      sessionStorage.setItem(googleLoginStorageKey, JSON.stringify({ state, browser_proof }))
      window.location.assign(authorization_url)
    } catch {
      if (signal.aborted) return
      setError('root', { message: 'Google 로그인 요청에 실패했습니다. 다시 시도해 주세요.' })
      googleRequest.current = false
      setGooglePending(false)
    }
  }

  const submit = handleSubmit(async (credentials) => {
    try {
      await onLogin(credentials)
    } catch (error) {
      const rejected = isHTTPError(error) && [401, 403].includes(error.response.status)
      setError('root', {
        message: rejected
          ? '이메일 또는 비밀번호를 확인해 주세요.'
          : '로그인 요청에 실패했습니다. 다시 시도해 주세요.',
      })
    }
  })

  return (
    <form className="w-full" onSubmit={submit} noValidate>
      <Card className="gap-6 py-8 shadow-none">
        <CardHeader className="items-center gap-2 text-center">
          <h1 className="text-2xl font-bold tracking-tight sm:text-3xl">로그인</h1>
          <p className="text-sm leading-6 text-muted-foreground">
            Conbira 계정으로 로그인하여 서비스를 이용하세요.
          </p>
        </CardHeader>
        <CardContent className="grid gap-4">
          <div className="grid gap-2">
            <label className="text-sm font-medium" htmlFor="login-email">
              이메일
            </label>
            <Input
              id="login-email"
              placeholder="이메일을 입력하세요."
              type="email"
              autoComplete="username"
              aria-invalid={!!errors.email}
              aria-describedby={errors.email ? 'login-email-error' : undefined}
              required
              {...register('email')}
            />
            {errors.email && (
              <p id="login-email-error" className="text-xs text-destructive" role="alert">
                {errors.email.message}
              </p>
            )}
          </div>
          <div className="grid gap-2">
            <label className="text-sm font-medium" htmlFor="login-password">
              비밀번호
            </label>
            <Input
              id="login-password"
              placeholder="비밀번호를 입력하세요."
              type="password"
              autoComplete="current-password"
              aria-invalid={!!errors.password}
              aria-describedby={errors.password ? 'login-password-error' : undefined}
              required
              {...register('password')}
            />
            {errors.password && (
              <p id="login-password-error" className="text-xs text-destructive" role="alert">
                {errors.password.message}
              </p>
            )}
          </div>
          {errors.root && (
            <Alert variant="destructive">
              <AlertDescription>{errors.root.message}</AlertDescription>
            </Alert>
          )}
        </CardContent>
        <CardFooter className="grid gap-4">
          <Button className="w-full" type="submit" disabled={isSubmitting || googlePending}>
            {isSubmitting ? '로그인 중…' : '로그인'}
          </Button>
          <div className="flex items-center gap-3 text-xs text-muted-foreground" aria-hidden="true">
            <span className="h-px flex-1 bg-border" />
            또는
            <span className="h-px flex-1 bg-border" />
          </div>
          <Button
            className="w-full"
            type="button"
            variant="outline"
            disabled={googlePending || isSubmitting}
            onClick={startGoogle}
          >
            {googlePending ? 'Google 로그인 중…' : 'Google로 로그인'}
          </Button>
          <div className="text-center">{cancel}</div>
        </CardFooter>
      </Card>
    </form>
  )
}
