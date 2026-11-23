import { Link, Outlet } from '@tanstack/react-router'
import { adminSections } from '@/features/rbac'

export function AdminLayout() {
  return (
    <main className="grid min-h-90 grid-rows-[auto_1fr] sm:grid-cols-[10rem_minmax(0,1fr)] sm:grid-rows-1">
      <nav aria-label="관리 메뉴" className="border-b bg-muted/30 sm:border-r sm:border-b-0 sm:py-4">
        <ul className="grid grid-cols-3 sm:grid-cols-1">
          {[{ to: '/admin', label: '관리 홈' } as const, ...adminSections].map(({ to, label }) => (
            <li key={to}>
              <Link to={to} activeOptions={{ exact: true }}
                className="flex min-h-11 items-center justify-center border-b-2 border-transparent px-2 text-sm text-muted-foreground hover:text-foreground focus-visible:outline-2 focus-visible:-outline-offset-4 focus-visible:outline-ring aria-[current=page]:border-primary aria-[current=page]:font-semibold aria-[current=page]:text-foreground sm:justify-start sm:border-s-2 sm:border-b-0 sm:px-4">
                {label}
              </Link>
            </li>
          ))}
        </ul>
      </nav>
      <Outlet />
    </main>
  )
}
