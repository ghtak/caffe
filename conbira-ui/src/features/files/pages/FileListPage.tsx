import { zodResolver } from '@hookform/resolvers/zod'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useEffect, useRef, useState } from 'react'
import { useForm } from 'react-hook-form'
import { z } from 'zod'
import { Button } from '@/shared/components/ui/Button'
import { Input } from '@/shared/components/ui/Input'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/shared/components/ui/Table'
import {
  fileChangeOptions,
  fileDownloadOptions,
  fileErrorMessage,
  fileKeys,
  fileListOptions,
} from '../api/files'
import type { FileListPageProps } from '../types/files.types'

const uploadSchema = z.object({
  files: z.custom<FileList>(
    (value) => value instanceof FileList && value.length > 0,
    '파일을 선택하세요.',
  ),
  groupId: z.union([z.literal(''), z.uuid('올바른 그룹 ID를 입력하세요.')]),
})
const attachSchema = z.object({
  resource_type: z.string().trim().min(1, '리소스 유형을 입력하세요.'),
  resource_key: z.string().trim().min(1, '리소스 키를 입력하세요.'),
})

export function FileListPage({ token, canManage }: FileListPageProps) {
  const client = useQueryClient()
  const [filters, setFilters] = useState({ name: '', page: 1 })
  const search = useForm<{ name: string }>({ defaultValues: { name: '' } })
  const list = useQuery(fileListOptions(token, filters))
  const [target, setTarget] = useState<{ action: 'attach' | 'delete'; id: string } | null>(null)
  const [uploadOpen, setUploadOpen] = useState(false)
  const [notice, setNotice] = useState('')
  const selected = list.data?.items.find((file) => file.file_id === target?.id)
  const attach = useForm<z.infer<typeof attachSchema>>({
    resolver: zodResolver(attachSchema),
    defaultValues: { resource_type: '', resource_key: '' },
  })
  const change = useMutation({
    ...fileChangeOptions(token),
    onSuccess: async (_, command) => {
      setNotice(
        command.action === 'upload'
          ? '파일을 업로드했습니다.'
          : command.action === 'attach'
            ? '파일을 연결했습니다.'
            : '파일을 삭제했습니다.',
      )
      if (command.action === 'upload') setUploadOpen(false)
      setTarget(null)
      await client.invalidateQueries({ queryKey: fileKeys.all })
    },
  })
  const download = useMutation({
    ...fileDownloadOptions(token),
    onSuccess: (data) => {
      window.location.assign(data.url)
    },
  })
  const busy = change.isPending || download.isPending
  const error = change.error ?? download.error

  return (
    <section className="mx-auto grid w-full max-w-7xl min-w-0 content-start gap-8 p-5 sm:p-8 lg:p-12">
      <header className="grid gap-3">
        <p className="text-sm font-medium text-primary">관리 / 파일 관리</p>
        <h1 className="text-3xl font-bold tracking-tight">파일 관리</h1>
        <p className="text-sm text-muted-foreground">
          파일 목록을 확인하고 업로드, 다운로드, 연결과 삭제를 관리합니다.
        </p>
      </header>
      {notice && <p role="status">{notice}</p>}
      {!uploadOpen && error && (
        <p role="alert" className="text-destructive">
          {fileErrorMessage(error)}
        </p>
      )}
      <form
        aria-label="파일 검색"
        className="flex flex-wrap items-end gap-3"
        onSubmit={search.handleSubmit(({ name }) => {
          setFilters({ name: name.trim(), page: 1 })
          setTarget(null)
        })}
      >
        <label className="grid flex-1 gap-2 text-sm">
          파일명
          <Input {...search.register('name')} placeholder="파일 이름 검색" disabled={busy} />
        </label>
        <Button type="submit" disabled={busy}>
          검색
        </Button>
      </form>
      <section aria-label="파일 목록" className="grid min-w-0 gap-4">
        <div className="flex items-center justify-between gap-4">
          <h2 className="font-semibold">파일 목록</h2>
          <div className="flex items-center gap-2">
            {canManage && (
              <Button
                type="button"
                disabled={busy}
                onClick={() => {
                  setNotice('')
                  change.reset()
                  download.reset()
                  setUploadOpen(true)
                }}
              >
                파일 업로드
              </Button>
            )}
            <Button
              type="button"
              variant="outline"
              disabled={list.isFetching || busy}
              onClick={() => void list.refetch()}
            >
              새로고침
            </Button>
          </div>
        </div>
        {list.isPending ? (
          <p role="status">목록을 불러오는 중…</p>
        ) : list.isError ? (
          <div className="grid justify-items-start gap-3">
            <p role="alert">{fileErrorMessage(list.error)}</p>
            <Button variant="outline" onClick={() => void list.refetch()}>
              다시 시도
            </Button>
          </div>
        ) : list.data.items.length === 0 ? (
          <p className="rounded-lg border border-dashed p-5 text-sm text-muted-foreground">
            {filters.name ? '검색 조건에 맞는 파일이 없습니다.' : '등록된 파일이 없습니다.'}
          </p>
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead scope="col">파일명</TableHead>
                <TableHead scope="col">상태</TableHead>
                <TableHead scope="col">크기</TableHead>
                <TableHead scope="col">그룹 ID</TableHead>
                <TableHead scope="col">등록일</TableHead>
                {canManage && <TableHead scope="col">작업</TableHead>}
              </TableRow>
            </TableHeader>
            <TableBody>
              {list.data.items.map((file) => (
                <TableRow key={file.file_id} data-testid={`files-row-${file.file_id}`}>
                  <TableCell className="max-w-80 break-all whitespace-normal">
                    {file.original_filename}
                  </TableCell>
                  <TableCell>
                    {file.status === 'pending'
                      ? '연결 대기'
                      : file.status === 'attached'
                        ? '연결됨'
                        : '삭제됨'}
                  </TableCell>
                  <TableCell>{file.size_bytes.toLocaleString('ko-KR')} B</TableCell>
                  <TableCell className="font-mono text-xs">{file.group_id}</TableCell>
                  <TableCell>{new Date(file.created_at).toLocaleString('ko-KR')}</TableCell>
                  {canManage && (
                    <TableCell>
                      <div className="flex gap-2">
                        <Button
                          size="sm"
                          variant="outline"
                          disabled={busy}
                          aria-label={`${file.original_filename} 다운로드`}
                          onClick={() => {
                            change.reset()
                            download.mutate(file.file_id)
                          }}
                        >
                          다운로드
                        </Button>
                        {file.status === 'pending' && (
                          <Button
                            size="sm"
                            variant="outline"
                            disabled={busy}
                            aria-label={`${file.original_filename} 연결`}
                            onClick={() => {
                              setTarget({ action: 'attach', id: file.file_id })
                              attach.reset()
                              change.reset()
                              download.reset()
                              setNotice('')
                            }}
                          >
                            연결 (attach)
                          </Button>
                        )}
                        <Button
                          size="sm"
                          variant="destructive"
                          disabled={busy}
                          aria-label={`${file.original_filename} 삭제`}
                          onClick={() => {
                            setTarget({ action: 'delete', id: file.file_id })
                            change.reset()
                            download.reset()
                            setNotice('')
                          }}
                        >
                          삭제
                        </Button>
                      </div>
                    </TableCell>
                  )}
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </section>
      <nav aria-label="파일 목록 페이지" className="flex items-center justify-center gap-3">
        <Button
          variant="outline"
          aria-disabled={filters.page === 1 || list.isFetching || busy}
          className="aria-disabled:opacity-50"
          onClick={() => {
            if (filters.page > 1 && !list.isFetching && !busy) {
              setFilters({ ...filters, page: filters.page - 1 })
              setTarget(null)
            }
          }}
        >
          이전
        </Button>
        <span className="text-sm" aria-live="polite">
          {filters.page} 페이지
        </span>
        <Button
          variant="outline"
          aria-disabled={!list.data?.has_next || list.isFetching || busy}
          className="aria-disabled:opacity-50"
          onClick={() => {
            if (list.data?.has_next && !list.isFetching && !busy) {
              setFilters({ ...filters, page: filters.page + 1 })
              setTarget(null)
            }
          }}
        >
          다음
        </Button>
      </nav>
      {canManage && selected && target?.action === 'attach' && (
        <form
          aria-label="파일 연결"
          className="grid gap-4 rounded-lg border p-5"
          onSubmit={attach.handleSubmit((values) => {
            change.mutate({
              action: 'attach',
              groupId: selected.group_id,
              body: { ...values, attach_file_ids: [selected.file_id], detach_file_ids: [] },
            })
          })}
        >
          <h2 className="font-semibold">파일 연결: {selected.original_filename}</h2>
          <div className="grid gap-4 sm:grid-cols-2">
            <label className="grid gap-2 text-sm">
              리소스 유형
              <Input {...attach.register('resource_type')} placeholder="예: post" disabled={busy} />
            </label>
            <label className="grid gap-2 text-sm">
              리소스 키
              <Input {...attach.register('resource_key')} placeholder="예: 42" disabled={busy} />
            </label>
          </div>
          {attach.formState.errors.resource_type && (
            <p role="alert">{attach.formState.errors.resource_type.message}</p>
          )}
          {attach.formState.errors.resource_key && (
            <p role="alert">{attach.formState.errors.resource_key.message}</p>
          )}
          <div className="flex gap-2">
            <Button type="submit" disabled={busy}>
              연결
            </Button>
            <Button type="button" variant="outline" disabled={busy} onClick={() => setTarget(null)}>
              취소
            </Button>
          </div>
        </form>
      )}
      {canManage && selected && target?.action === 'delete' && (
        <section aria-label="파일 삭제 확인" className="grid gap-4 rounded-lg border p-5">
          <h2 className="font-semibold">{selected.original_filename} 파일을 삭제하시겠습니까?</h2>
          <p className="text-sm text-muted-foreground">삭제한 파일은 복원할 수 없습니다.</p>
          <div className="flex gap-2">
            <Button
              variant="destructive"
              disabled={busy}
              onClick={() => change.mutate({ action: 'delete', fileId: selected.file_id })}
            >
              삭제 확인
            </Button>
            <Button variant="outline" disabled={busy} onClick={() => setTarget(null)}>
              취소
            </Button>
          </div>
        </section>
      )}
      {canManage && uploadOpen && (
        <FileUploadDialog
          busy={busy}
          uploading={change.isPending && change.variables.action === 'upload'}
          error={change.error}
          onClose={() => {
            if (!busy) {
              change.reset()
              setUploadOpen(false)
            }
          }}
          onSubmit={(values) => {
            setNotice('')
            download.reset()
            change.mutate({ action: 'upload', ...values })
          }}
        />
      )}
    </section>
  )
}

function FileUploadDialog({
  busy,
  uploading,
  error,
  onClose,
  onSubmit,
}: {
  busy: boolean
  uploading: boolean
  error: Error | null
  onClose: () => void
  onSubmit: (values: z.infer<typeof uploadSchema>) => void
}) {
  const dialog = useRef<HTMLDialogElement>(null)
  const form = useForm<z.infer<typeof uploadSchema>>({
    resolver: zodResolver(uploadSchema),
    defaultValues: { groupId: '' },
  })
  useEffect(() => {
    const element = dialog.current
    if (!element) return
    if (typeof element.showModal === 'function') element.showModal()
    else element.setAttribute('open', '')
    return () => {
      if (element.open && typeof element.close === 'function') element.close()
    }
  }, [])
  return (
    <dialog
      ref={dialog}
      aria-labelledby="file-upload-title"
      onCancel={(event) => {
        event.preventDefault()
        onClose()
      }}
      className="fixed inset-0 m-auto max-h-[min(90vh,42rem)] w-[calc(100%-2rem)] max-w-lg overflow-hidden rounded-xl border bg-card p-0 text-card-foreground shadow-xl backdrop:bg-black/50"
    >
      <div className="flex max-h-[min(90vh,42rem)] flex-col">
        <header className="flex items-start justify-between gap-4 border-b p-5">
          <div>
            <h2 id="file-upload-title" className="text-lg font-semibold">
              파일 업로드
            </h2>
            <p className="mt-1 text-sm text-muted-foreground">
              업로드할 파일과 그룹 ID를 선택하세요.
            </p>
          </div>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            disabled={busy}
            onClick={onClose}
            aria-label="파일 업로드 닫기"
          >
            닫기
          </Button>
        </header>
        <form
          aria-label="파일 업로드"
          className="grid gap-4 overflow-y-auto p-5"
          onSubmit={form.handleSubmit(onSubmit)}
        >
          {error && (
            <p role="alert" className="text-sm text-destructive">
              {fileErrorMessage(error)}
            </p>
          )}
          <label className="grid gap-2 text-sm">
            업로드할 파일
            <Input type="file" multiple {...form.register('files')} disabled={busy} />
          </label>
          {form.formState.errors.files && (
            <p role="alert" className="text-sm text-destructive">
              {form.formState.errors.files.message}
            </p>
          )}
          <label className="grid gap-2 text-sm">
            그룹 ID (비워 두면 새 그룹)
            <Input
              {...form.register('groupId')}
              placeholder="기존 그룹에 추가할 때 입력"
              disabled={busy}
            />
          </label>
          {form.formState.errors.groupId && (
            <p role="alert" className="text-sm text-destructive">
              {form.formState.errors.groupId.message}
            </p>
          )}
          <div className="flex justify-end gap-2 pt-2">
            <Button type="button" variant="outline" disabled={busy} onClick={onClose}>
              취소
            </Button>
            <Button type="submit" disabled={busy}>
              {uploading ? '업로드 중…' : '업로드'}
            </Button>
          </div>
        </form>
      </div>
    </dialog>
  )
}
