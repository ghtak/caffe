import { EmptyState } from '@/shared/components/ui/EmptyState'
import { PageHeader } from '@/shared/components/ui/PageHeader'

export function HomePage() {
  return (
    <main className="mx-auto flex w-full max-w-6xl flex-col gap-6 px-4 py-6 sm:px-6 sm:py-8 lg:px-8">
      <PageHeader title="홈" description="커뮤니티와 관리 기능을 이용할 수 있습니다." />
      <EmptyState>서비스 콘텐츠 영역</EmptyState>
    </main>
  )
}
