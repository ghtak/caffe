import { CatalogList } from '../components/CatalogList'
import type { Catalog } from '../types/catalog.types'

export function AdminCatalogPage({ catalog, token }: { catalog: Catalog; token: string }) {
  return (
    <section className="mx-auto grid w-full max-w-7xl min-w-0 content-start gap-8 p-5 sm:p-8 lg:p-12">
      <header className="grid gap-3">
        <p className="text-sm font-medium text-primary">
          관리 / {catalog === 'roles' ? '역할 관리' : '권한 관리'}
        </p>
        <h1 className="text-3xl font-bold tracking-tight">
          {catalog === 'roles' ? '역할 관리' : '권한 관리'}
        </h1>
        <p className="text-sm leading-6 text-muted-foreground sm:text-base">
          {catalog === 'roles'
            ? '계정에 부여할 수 있는 역할과 설명을 확인합니다.'
            : '권한 코드와 각 권한이 허용하는 작업을 확인합니다.'}
        </p>
      </header>
      <CatalogList catalog={catalog} token={token} />
    </section>
  )
}
