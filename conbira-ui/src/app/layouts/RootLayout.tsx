import { Link, Outlet, useNavigate } from '@tanstack/react-router'
import { Alert, AlertDescription } from '@/shared/components/ui/Alert'
import { Button } from '@/shared/components/ui/Button'
import { Card } from '@/shared/components/ui/Card'
import { visibleAdminSections } from '../access'
import { useSession } from '../providers/SessionProvider'
import { ThemeSelect } from './ThemeSelect'

export function RootLayout() {
  const { session, signOut, retry } = useSession()
  const navigate = useNavigate()

  async function logout() {
    try {
      await navigate({ to: '/', replace: true })
    } finally {
      signOut()
    }
  }

  return (
    <div className="min-h-screen">
      <Card className="min-h-screen gap-0 overflow-hidden rounded-none border-0 bg-background py-0 shadow-none">
        <header className="flex min-h-18 flex-wrap items-center justify-between gap-x-3 gap-y-2 border-b bg-card px-5 py-2 sm:px-8">
          <Button asChild variant="ghost" className="px-0 text-xl font-bold tracking-tight hover:bg-transparent dark:hover:bg-transparent">
            <Link to="/" aria-label="Conbira 홈">Conbira</Link>
          </Button>
          <nav aria-label="상단 메뉴" className="ml-auto flex flex-wrap items-center justify-end gap-2">
            {session.status === 'guest' && (
              <Button asChild>
                <Link to="/login" activeOptions={{ exact: true }}>로그인</Link>
              </Button>
            )}
            {session.status === 'authenticated' && visibleAdminSections(session.permissions).length > 0 && (
              <Button asChild variant="secondary">
                <Link to="/admin">관리</Link>
              </Button>
            )}
            {(session.status === 'authenticated' || session.status === 'error') && (
              <Button variant="outline" type="button" onClick={() => void logout()}>
                로그아웃
              </Button>
            )}
            <ThemeSelect />
          </nav>
        </header>
        {session.status === 'loading' && (
          <p className="border-b px-4 py-3 text-sm text-muted-foreground sm:px-6" role="status">로그인 상태 확인 중…</p>
        )}
        {session.status === 'error' && (
          <div className="grid justify-items-start gap-2 border-b px-4 py-3 sm:px-6">
            <Alert variant="destructive">
              <AlertDescription>로그인 상태를 확인하지 못했습니다.</AlertDescription>
            </Alert>
            <Button variant="outline" type="button" onClick={retry}>다시 시도</Button>
          </div>
        )}
        <Outlet />
      </Card>
    </div>
  )
}
