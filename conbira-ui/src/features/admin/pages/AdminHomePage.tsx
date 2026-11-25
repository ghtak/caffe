import { Link } from '@tanstack/react-router'
import { adminSections } from '../admin-sections'

export function AdminHomePage({
  sections,
}: {
  sections: readonly (typeof adminSections)[number][]
}) {
  return (
    <section className="mx-auto grid w-full max-w-7xl content-start gap-8 p-5 sm:p-8 lg:p-12">
      <header className="grid gap-3">
        <p className="text-sm font-medium text-primary">관리</p>
        <h1 className="text-3xl font-bold tracking-tight">관리 홈</h1>
        <p className="text-sm text-muted-foreground sm:text-base">
          사용자, 역할과 권한을 확인할 영역을 선택하세요.
        </p>
      </header>
      <ul className="grid gap-4 sm:grid-cols-2">
        {sections.map(({ to, label, description }) => (
          <li key={to}>
            <Link
              to={to}
              className="group grid min-h-36 grid-cols-[minmax(0,1fr)_auto] items-center gap-4 rounded-lg border bg-card p-6 transition-colors hover:border-primary/50 hover:bg-accent/30 focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-ring"
            >
              <span className="grid gap-1">
                <span className="font-medium group-hover:underline group-hover:underline-offset-4">
                  {label}
                </span>
                <span className="text-sm text-muted-foreground">{description}</span>
              </span>
              <span aria-hidden="true" className="text-xl text-primary">
                →
              </span>
            </Link>
          </li>
        ))}
      </ul>
    </section>
  )
}
