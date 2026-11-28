import { PageHeader } from '@/shared/components/ui/PageHeader'
import { CatalogList } from '../components/CatalogList'
import type { Catalog } from '../types/catalog.types'

export function AdminCatalogPage({ catalog, token }: { catalog: Catalog; token: string }) {
  const title = catalog === 'roles' ? '역할 관리' : '권한 관리'
  return (
    <section className="mx-auto flex w-full max-w-6xl min-w-0 flex-col gap-6 px-4 py-6 sm:px-6 sm:py-8 lg:px-8">
      <PageHeader
        eyebrow={`관리 / ${title}`}
        title={title}
        description={
          catalog === 'roles'
            ? '계정에 부여할 수 있는 역할과 설명을 확인합니다.'
            : '권한 코드와 각 권한이 허용하는 작업을 확인합니다.'
        }
      />
      <CatalogList catalog={catalog} token={token} />
    </section>
  )
}
