import { zodResolver } from '@hookform/resolvers/zod'
import { isHTTPError } from 'ky'
import type { ReactNode } from 'react'
import { useForm } from 'react-hook-form'
import { Alert, AlertDescription } from '@/shared/components/ui/Alert'
import { Button } from '@/shared/components/ui/Button'
import { Card, CardContent, CardFooter, CardHeader } from '@/shared/components/ui/Card'
import { Input } from '@/shared/components/ui/Input'
import { credentialsSchema, type Credentials } from '../types/auth.types'

export function LoginForm({ onLogin, cancel }: {
  onLogin: (credentials: Credentials) => Promise<void>
  cancel: ReactNode
}) {
  const { register, handleSubmit, setError, formState: { errors, isSubmitting } } =
    useForm<Credentials>({ resolver: zodResolver(credentialsSchema) })

  const submit = handleSubmit(async (credentials) => {
    try {
      await onLogin(credentials)
    } catch (error) {
      const rejected = isHTTPError(error) && [401, 403].includes(error.response.status)
      setError('root', { message: rejected
        ? '이메일 또는 비밀번호를 확인해 주세요.'
        : '로그인 요청에 실패했습니다. 다시 시도해 주세요.' })
    }
  })

  return (
    <form className="w-full" onSubmit={submit} noValidate>
      <Card className="gap-5 shadow-none">
        <CardHeader><h1 className="text-xl font-semibold">로그인</h1></CardHeader>
        <CardContent className="grid gap-4">
          <div className="grid gap-2">
            <label className="text-sm font-medium" htmlFor="login-email">이메일</label>
            <Input id="login-email" className="h-11" type="email" autoComplete="username"
              aria-invalid={!!errors.email} aria-describedby={errors.email ? 'login-email-error' : undefined}
              required {...register('email')} />
            {errors.email && <p id="login-email-error" className="text-sm text-destructive" role="alert">{errors.email.message}</p>}
          </div>
          <div className="grid gap-2">
            <label className="text-sm font-medium" htmlFor="login-password">비밀번호</label>
            <Input id="login-password" className="h-11" type="password" autoComplete="current-password"
              aria-invalid={!!errors.password} aria-describedby={errors.password ? 'login-password-error' : undefined}
              required {...register('password')} />
            {errors.password && <p id="login-password-error" className="text-sm text-destructive" role="alert">{errors.password.message}</p>}
          </div>
          {errors.root && (
            <Alert variant="destructive"><AlertDescription>{errors.root.message}</AlertDescription></Alert>
          )}
        </CardContent>
        <CardFooter className="flex-wrap justify-end gap-2">
          {cancel}
          <Button type="submit" disabled={isSubmitting}>
            {isSubmitting ? '로그인 중…' : '로그인'}
          </Button>
        </CardFooter>
      </Card>
    </form>
  )
}
