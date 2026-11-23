import { Link } from '@tanstack/react-router'
import { adminSections } from '../admin-sections'

export function AdminHomePage() {
  return (
    <section className="grid content-start gap-4 p-4 sm:p-6">
      <h1 className="text-xl font-semibold">관리 홈</h1>
      <ul className="divide-y border-y">
        {adminSections.map(({ to, label, description }) => (
          <li key={to}>
            <Link to={to}
              className="group grid grid-cols-[minmax(0,1fr)_auto] items-center gap-4 px-1 py-4 focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-ring">
              <span className="grid gap-1">
                <span className="font-medium group-hover:underline group-hover:underline-offset-4">{label}</span>
                <span className="text-sm text-muted-foreground">{description}</span>
              </span>
              <span aria-hidden="true" className="text-muted-foreground">→</span>
            </Link>
          </li>
        ))}
      </ul>
    </section>
  )
}
