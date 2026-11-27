import { Link, Outlet } from '@tanstack/react-router'

export function CommunityLayout() {
  return (
    <main className="grid min-h-[calc(100vh-4.5rem)] grid-rows-[auto_1fr] md:grid-cols-[15rem_minmax(0,1fr)] md:grid-rows-1">
      <nav
        aria-label="커뮤니티 메뉴"
        className="border-b bg-card md:border-r md:border-b-0 md:px-3 md:py-8"
      >
        <ul className="grid grid-cols-2 gap-1 p-2 md:grid-cols-1 md:p-0">
          {[
            { to: '/community/notice', label: '공지' } as const,
            { to: '/community/free', label: '자유게시판' } as const,
          ].map(({ to, label }) => (
            <li key={to}>
              <Link
                to={to}
                activeOptions={{ exact: true }}
                className="flex min-h-11 items-center justify-center rounded-md px-2 text-sm font-medium text-muted-foreground transition-colors hover:bg-muted hover:text-foreground focus-visible:outline-2 focus-visible:-outline-offset-4 focus-visible:outline-ring aria-[current=page]:bg-accent aria-[current=page]:font-semibold aria-[current=page]:text-primary md:justify-start md:px-4"
              >
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
