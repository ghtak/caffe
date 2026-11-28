import { Link } from '@tanstack/react-router'
import { useQuery } from '@tanstack/react-query'
import { isHTTPError } from 'ky'
import { Alert, AlertDescription } from '@/shared/components/ui/Alert'
import { Button } from '@/shared/components/ui/Button'
import { EmptyState } from '@/shared/components/ui/EmptyState'
import {
  Table,
  TableBody,
  TableCaption,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/shared/components/ui/Table'
import { catalogOptions } from '../api/catalog'
import type { Catalog } from '../types/catalog.types'

function catalogErrorMessage(error: unknown) {
  if (isHTTPError(error)) {
    if (error.response.status === 403) return '목록을 조회할 권한이 없습니다.'
    if (error.response.status === 401) return '로그인 세션이 만료되었습니다.'
  }
  return '목록을 불러오지 못했습니다.'
}

export function CatalogList({ catalog, token }: { catalog: Catalog; token: string }) {
  const label = catalog === 'roles' ? '역할' : '권한'
  const list = useQuery(catalogOptions(catalog, token))

  return (
    <div className="w-full min-w-0">
      {list.isPending ? (
        <div role="status" className="rounded-lg border bg-card p-6 text-sm text-muted-foreground">
          목록을 불러오는 중…
        </div>
      ) : list.isError ? (
        <div className="grid justify-items-start gap-2">
          <Alert variant="destructive">
            <AlertDescription>{catalogErrorMessage(list.error)}</AlertDescription>
          </Alert>
          <Button
            variant="outline"
            type="button"
            disabled={list.isFetching}
            onClick={() => void list.refetch()}
          >
            다시 시도
          </Button>
        </div>
      ) : list.data.length === 0 ? (
        <EmptyState>등록된 {label}이 없습니다.</EmptyState>
      ) : (
        <Table className="caption-top wrap-anywhere">
          <TableCaption className="m-0 border-b px-4 py-3 text-left font-semibold text-foreground">
            {label} 목록
          </TableCaption>
          <TableHeader>
            <TableRow>
              <TableHead className="px-4" scope="col">
                코드
              </TableHead>
              <TableHead className="px-4" scope="col">
                이름
              </TableHead>
              <TableHead className="px-4" scope="col">
                설명
              </TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {list.data.map(({ id, code, name, description }) => (
              <TableRow key={code}>
                <TableCell className="px-4 py-3 whitespace-normal">
                  {catalog === 'roles' ? (
                    <Link
                      className="text-primary underline-offset-4 hover:underline"
                      to="/admin/roles/$roleId"
                      params={{ roleId: String(id) }}
                    >
                      <code>{code}</code>
                    </Link>
                  ) : (
                    <code>{code}</code>
                  )}
                </TableCell>
                <TableCell className="px-4 py-3 font-semibold whitespace-normal">{name}</TableCell>
                <TableCell className="px-4 py-3 whitespace-normal text-muted-foreground">
                  {description}
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      )}
    </div>
  )
}
