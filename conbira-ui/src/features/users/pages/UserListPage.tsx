import { useQuery } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import { isHTTPError } from 'ky'
import { useState } from 'react'
import { Alert, AlertDescription } from '@/shared/components/ui/Alert'
import { Button } from '@/shared/components/ui/Button'
import { EmptyState } from '@/shared/components/ui/EmptyState'
import { Input } from '@/shared/components/ui/Input'
import { PageHeader } from '@/shared/components/ui/PageHeader'
import { Select } from '@/shared/components/ui/Select'
import {
  Table,
  TableBody,
  TableCaption,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/shared/components/ui/Table'
import { statusLabels, userListOptions } from '../api/users'

export function UserListPage({ token }: { token: string }) {
  const [name, setName] = useState('')
  const [email, setEmail] = useState('')
  const [status, setStatus] = useState('')
  const [filters, setFilters] = useState({ name: '', email: '', status: '', page: 1 })
  const list = useQuery(userListOptions(token, filters))
  const errorMessage =
    isHTTPError(list.error) && list.error.response.status === 403
      ? '목록을 조회할 권한이 없습니다.'
      : isHTTPError(list.error) && list.error.response.status === 401
        ? '로그인 세션이 만료되었습니다.'
        : '목록을 불러오지 못했습니다.'
  return (
    <section className="mx-auto flex w-full max-w-6xl min-w-0 flex-col gap-6 px-4 py-6 sm:px-6 sm:py-8 lg:px-8">
      <PageHeader
        eyebrow="관리 / 사용자 목록"
        title="사용자 목록"
        description="등록된 사용자와 계정 상태를 확인합니다."
      />
      <form
        className="grid items-end gap-4 sm:grid-cols-2 lg:grid-cols-[1fr_1fr_1fr_auto]"
        onSubmit={(event) => {
          event.preventDefault()
          setFilters({ name: name.trim(), email: email.trim(), status, page: 1 })
        }}
      >
        <label className="grid gap-2 text-sm font-medium">
          이름
          <Input
            value={name}
            onChange={(event) => setName(event.target.value)}
            placeholder="닉네임 검색"
          />
        </label>
        <label className="grid gap-2 text-sm font-medium">
          이메일
          <Input
            value={email}
            onChange={(event) => setEmail(event.target.value)}
            placeholder="이메일 검색"
          />
        </label>
        <label className="grid gap-2 text-sm font-medium">
          상태
          <Select value={status} onChange={(event) => setStatus(event.target.value)}>
            <option value="">전체</option>
            {Object.entries(statusLabels).map(([value, label]) => (
              <option key={value} value={value}>
                {label}
              </option>
            ))}
          </Select>
        </label>
        <Button type="submit">검색</Button>
      </form>
      {list.isPending ? (
        <div role="status" className="rounded-lg border bg-card p-6 text-sm text-muted-foreground">
          목록을 불러오는 중…
        </div>
      ) : list.isError ? (
        <div className="grid justify-items-start gap-2">
          <Alert variant="destructive">
            <AlertDescription>{errorMessage}</AlertDescription>
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
      ) : list.data.items.length === 0 ? (
        <EmptyState>
          {filters.name || filters.email || filters.status
            ? '검색 조건에 맞는 사용자가 없습니다.'
            : '등록된 사용자가 없습니다.'}
        </EmptyState>
      ) : (
        <Table className="caption-top wrap-anywhere">
          <TableCaption className="m-0 border-b px-4 py-3 text-left font-semibold text-foreground">
            사용자 목록
          </TableCaption>
          <TableHeader>
            <TableRow>
              <TableHead className="px-4" scope="col">
                ID
              </TableHead>
              <TableHead className="px-4" scope="col">
                닉네임
              </TableHead>
              <TableHead className="px-4" scope="col">
                이메일
              </TableHead>
              <TableHead className="px-4" scope="col">
                상태
              </TableHead>
              <TableHead className="px-4" scope="col">
                가입일
              </TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {list.data.items.map((user) => (
              <TableRow key={user.id}>
                <TableCell className="px-4 py-3">{user.id}</TableCell>
                <TableCell className="px-4 py-3 font-semibold">
                  <Link
                    to="/admin/users/$userId"
                    params={{ userId: String(user.id) }}
                    className="text-primary underline-offset-4 hover:underline"
                  >
                    {user.nickname}
                  </Link>
                </TableCell>
                <TableCell className="px-4 py-3 whitespace-normal">
                  {user.emails.length ? user.emails.join(', ') : '—'}
                </TableCell>
                <TableCell className="px-4 py-3">{statusLabels[user.status]}</TableCell>
                <TableCell className="px-4 py-3">
                  {new Date(user.created_at).toLocaleDateString('ko-KR')}
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      )}
      <nav aria-label="사용자 목록 페이지" className="flex items-center justify-center gap-3">
        <Button
          variant="outline"
          size="sm"
          type="button"
          className="aria-disabled:opacity-50"
          aria-disabled={filters.page === 1 || list.isFetching}
          onClick={() => {
            if (filters.page > 1 && !list.isFetching)
              setFilters({ ...filters, page: filters.page - 1 })
          }}
        >
          이전
        </Button>
        <span className="text-sm text-muted-foreground" aria-live="polite">
          {filters.page} 페이지
        </span>
        <Button
          variant="outline"
          size="sm"
          type="button"
          className="aria-disabled:opacity-50"
          aria-disabled={!list.data?.has_next || list.isFetching}
          onClick={() => {
            if (list.data?.has_next && !list.isFetching)
              setFilters({ ...filters, page: filters.page + 1 })
          }}
        >
          다음
        </Button>
      </nav>
    </section>
  )
}
