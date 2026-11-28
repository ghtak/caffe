import { Link, Outlet, useNavigate } from '@tanstack/react-router'
import { Alert, AlertDescription } from '@/shared/components/ui/Alert'
import { Button } from '@/shared/components/ui/Button'
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
      await signOut()
    }
  }

  return (
    <div className="flex min-h-screen flex-col bg-background text-foreground">
      <header className="flex min-h-14 flex-wrap items-center justify-between gap-3 border-b bg-card px-4 py-2 sm:px-6">
        <Button asChild variant="ghost" size="sm" className="px-2 text-lg font-bold tracking-tight">
          <Link to="/" aria-label="Conbira 홈">
            Conbira
          </Link>
        </Button>
        <nav
          aria-label="상단 메뉴"
          className="ml-auto flex flex-wrap items-center justify-end gap-2"
        >
          <Button asChild variant="ghost" size="sm">
            <Link
              to="/community"
              className="aria-[current=page]:bg-accent aria-[current=page]:text-primary"
            >
              커뮤니티
            </Link>
          </Button>
          {session.status === 'guest' && (
            <Button asChild size="sm">
              <Link to="/login" activeOptions={{ exact: true }}>
                로그인
              </Link>
            </Button>
          )}
          {session.status === 'authenticated' &&
            visibleAdminSections(session.permissions).length > 0 && (
              <Button asChild variant="ghost" size="sm">
                <Link
                  to="/admin"
                  className="aria-[current=page]:bg-accent aria-[current=page]:text-primary"
                >
                  관리
                </Link>
              </Button>
            )}
          {(session.status === 'authenticated' || session.status === 'error') && (
            <Button variant="outline" size="sm" type="button" onClick={() => void logout()}>
              로그아웃
            </Button>
          )}
          <ThemeSelect />
        </nav>
      </header>
      {session.status === 'loading' && (
        <p
          className="border-b bg-card px-4 py-2.5 text-sm text-muted-foreground sm:px-6"
          role="status"
        >
          로그인 상태 확인 중…
        </p>
      )}
      {session.status === 'error' && (
        <div className="grid justify-items-start gap-2 border-b bg-card px-4 py-3 sm:px-6">
          <Alert variant="destructive">
            <AlertDescription>
              {session.reason === 'logout'
                ? '로그아웃하지 못했습니다. 다시 시도해 주세요.'
                : '로그인 상태를 확인하지 못했습니다.'}
            </AlertDescription>
          </Alert>
          <Button variant="outline" size="sm" type="button" onClick={retry}>
            다시 시도
          </Button>
        </div>
      )}
      <div className="flex flex-1 flex-col">
        <Outlet />
      </div>
    </div>
  )
}
