import { Link } from '@tanstack/react-router'
import { PageHeader } from '@/shared/components/ui/PageHeader'
import { adminSections } from '../admin-sections'

export function AdminHomePage({
  sections,
}: {
  sections: readonly (typeof adminSections)[number][]
}) {
  return (
    <section className="mx-auto flex w-full max-w-6xl flex-col gap-6 px-4 py-6 sm:px-6 sm:py-8 lg:px-8">
      <PageHeader
        eyebrow="관리"
        title="관리 홈"
        description="사용자, 역할, 권한과 파일을 관리할 영역을 선택하세요."
      />
      <ul className="grid gap-4 sm:grid-cols-2">
        {sections.map(({ to, label, description }) => (
          <li key={to}>
            <Link
              to={to}
              className="group grid min-h-32 grid-cols-[minmax(0,1fr)_auto] items-center gap-4 rounded-lg border bg-card p-6 transition-colors hover:border-primary/50 hover:bg-accent/40 focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-ring"
            >
              <span className="grid gap-1">
                <span className="font-semibold group-hover:text-primary">{label}</span>
                <span className="text-sm text-muted-foreground">{description}</span>
              </span>
              <span aria-hidden="true" className="text-lg text-primary">
                →
              </span>
            </Link>
          </li>
        ))}
      </ul>
    </section>
  )
}
