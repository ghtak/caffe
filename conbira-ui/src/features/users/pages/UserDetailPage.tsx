import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import { isHTTPError } from 'ky'
import { useState } from 'react'
import { Alert, AlertDescription } from '@/shared/components/ui/Alert'
import { Button } from '@/shared/components/ui/Button'
import {
  statusLabels,
  userDetailOptions,
  userKeys,
  userStatusMutationOptions,
  userStatusSchema,
  type UserStatus,
} from '../api/users'

function UserStatusEditor({
  id,
  nickname,
  status,
  token,
  onSaved,
}: {
  id: number
  nickname: string
  status: UserStatus
  token: string
  onSaved: () => void
}) {
  const queryClient = useQueryClient()
  const mutation = useMutation({
    ...userStatusMutationOptions(token),
    onSuccess: async () => {
      onSaved()
      await queryClient.invalidateQueries({ queryKey: userKeys.all })
    },
    onError: (error) => {
      const message =
        isHTTPError(error) && error.response.status === 403
          ? '상태를 변경할 권한이 없습니다.'
          : isHTTPError(error) && error.response.status === 401
            ? '로그인 세션이 만료되었습니다.'
            : '상태를 저장하지 못했습니다. 다시 시도해 주세요.'
      window.alert(message)
    },
  })

  return (
    <div className="grid gap-2">
      <div className="relative w-fit">
        <select
          aria-label={`${nickname} 상태`}
          className="h-10 min-w-32 cursor-pointer appearance-none rounded-lg border border-input bg-card py-2 pr-10 pl-4 text-sm font-medium shadow-xs transition-colors hover:border-primary/60 hover:bg-accent/50 focus-visible:border-ring focus-visible:outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50 aria-disabled:cursor-wait aria-disabled:opacity-60"
          value={status}
          aria-disabled={mutation.isPending}
          onChange={(event) => {
            if (mutation.isPending) {
              event.currentTarget.value = status
              return
            }
            const nextStatus = userStatusSchema.parse(event.target.value)
            mutation.reset()
            if (
              nextStatus !== status &&
              window.confirm(
                `${nickname}님의 상태를 ${statusLabels[status]}에서 ${statusLabels[nextStatus]} 상태로 변경할까요?`,
              )
            ) {
              mutation.mutate({ id, status: nextStatus })
            }
          }}
        >
          {Object.entries(statusLabels).map(([value, label]) => (
            <option key={value} value={value}>
              {label}
            </option>
          ))}
        </select>
        <svg
          aria-hidden="true"
          viewBox="0 0 20 20"
          fill="none"
          className="pointer-events-none absolute top-1/2 right-3 size-4 -translate-y-1/2 text-muted-foreground"
        >
          <path
            d="m5 7.5 5 5 5-5"
            stroke="currentColor"
            strokeWidth="1.5"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
      </div>
    </div>
  )
}

export function UserDetailPage({
  id,
  token,
  canEdit,
}: {
  id: number
  token: string
  canEdit: boolean
}) {
  const [saved, setSaved] = useState(false)
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
      <div aria-live="polite">
        {saved && <p className="text-sm text-muted-foreground">상태를 저장했습니다.</p>}
      </div>
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
            <dd>
              {canEdit ? (
                <UserStatusEditor
                  id={id}
                  nickname={detail.data.nickname}
                  status={detail.data.status}
                  token={token}
                  onSaved={() => setSaved(true)}
                />
              ) : (
                statusLabels[detail.data.status]
              )}
            </dd>
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
