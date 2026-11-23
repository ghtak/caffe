import { CatalogList } from '../components/CatalogList'
import type { Catalog } from '../types/catalog.types'

export function AdminCatalogPage({ catalog, token }: { catalog: Catalog; token: string }) {
  return (
    <section className="grid min-w-0 content-start gap-4 p-4 sm:p-6">
      <h1 className="text-xl font-semibold">{catalog === 'roles' ? '역할 관리' : '권한 관리'}</h1>
      <CatalogList catalog={catalog} token={token} />
    </section>
  )
}
