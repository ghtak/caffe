import { useQuery } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import { isHTTPError } from 'ky'
import { Alert, AlertDescription } from '@/shared/components/ui/Alert'
import { Button } from '@/shared/components/ui/Button'
import { statusLabels, userDetailOptions } from '../api/users'

export function UserDetailPage({ id, token }: { id: number; token: string }) {
  const detail = useQuery(userDetailOptions(token, id))
  const status = isHTTPError(detail.error) ? detail.error.response.status : undefined
  const errorMessage =
    status === 403
      ? '사용자를 조회할 권한이 없습니다.'
      : status === 404
        ? '사용자를 찾을 수 없습니다.'
        : status === 401
          ? '로그인 세션이 만료되었습니다.'
          : '사용자 정보를 불러오지 못했습니다.'

  return (
    <section className="mx-auto grid w-full max-w-7xl min-w-0 content-start gap-8 p-5 sm:p-8 lg:p-12">
      <header className="grid gap-3">
        <Link to="/admin/users" className="w-fit text-sm text-primary hover:underline">
          사용자 목록으로
        </Link>
        <h1 className="text-3xl font-bold tracking-tight">사용자 상세</h1>
        <p className="text-sm text-muted-foreground">
          사용자 기본정보와 연결된 이메일을 확인합니다.
        </p>
      </header>
      {detail.isPending ? (
        <p role="status" className="text-sm text-muted-foreground">
          사용자 정보를 불러오는 중…
        </p>
      ) : detail.isError ? (
        <div className="grid justify-items-start gap-3">
          <Alert variant="destructive">
            <AlertDescription>{errorMessage}</AlertDescription>
          </Alert>
          {status !== 403 && status !== 404 && status !== 401 && (
            <Button
              variant="outline"
              disabled={detail.isFetching}
              onClick={() => void detail.refetch()}
            >
              다시 시도
            </Button>
          )}
        </div>
      ) : (
        <dl className="grid gap-6 rounded-lg border bg-card p-6 sm:grid-cols-2">
          <div className="grid gap-2">
            <dt className="text-sm text-muted-foreground">ID</dt>
            <dd>{detail.data.id}</dd>
          </div>
          <div className="grid gap-2">
            <dt className="text-sm text-muted-foreground">닉네임</dt>
            <dd className="font-semibold wrap-anywhere">{detail.data.nickname}</dd>
          </div>
          <div className="grid gap-2">
            <dt className="text-sm text-muted-foreground">이메일</dt>
            <dd className="wrap-anywhere">
              {detail.data.emails.length
                ? detail.data.emails.join(', ')
                : '연결된 이메일이 없습니다.'}
            </dd>
          </div>
          <div className="grid gap-2">
            <dt className="text-sm text-muted-foreground">상태</dt>
            <dd>{statusLabels[detail.data.status]}</dd>
          </div>
          <div className="grid gap-2">
            <dt className="text-sm text-muted-foreground">가입일</dt>
            <dd>{new Date(detail.data.created_at).toLocaleString('ko-KR')}</dd>
          </div>
          <div className="grid gap-2">
            <dt className="text-sm text-muted-foreground">수정일</dt>
            <dd>{new Date(detail.data.updated_at).toLocaleString('ko-KR')}</dd>
          </div>
        </dl>
      )}
    </section>
  )
}
