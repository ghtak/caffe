import { Link, Outlet } from '@tanstack/react-router'

export interface SidebarNavItem {
  to: string
  label: string
}

export function SidebarShell({
  label,
  items,
}: {
  label: string
  items: readonly SidebarNavItem[]
}) {
  return (
    <div className="grid flex-1 grid-rows-[auto_1fr] md:grid-cols-[15rem_minmax(0,1fr)] md:grid-rows-1">
      <nav
        aria-label={label}
        className="border-b bg-card px-3 py-2 md:border-r md:border-b-0 md:px-3 md:py-6"
      >
        <ul className="grid grid-cols-2 gap-1 md:grid-cols-1">
          {items.map((item) => (
            <li key={item.to}>
              <Link
                to={item.to}
                activeOptions={{ exact: true }}
                className="flex min-h-10 items-center justify-center rounded-md px-3 text-sm font-medium text-muted-foreground transition-colors hover:bg-muted hover:text-foreground focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-ring aria-[current=page]:bg-accent aria-[current=page]:font-semibold aria-[current=page]:text-primary md:justify-start"
              >
                {item.label}
              </Link>
            </li>
          ))}
        </ul>
      </nav>
      <main className="min-w-0 bg-background">
        <Outlet />
      </main>
    </div>
  )
}
