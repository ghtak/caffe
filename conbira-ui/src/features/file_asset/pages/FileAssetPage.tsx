import { Dialog } from '@/shared/components/ui/Dialog'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { Alert, AlertDescription } from '@/shared/components/ui/Alert'
import { Button } from '@/shared/components/ui/Button'
import { EmptyState } from '@/shared/components/ui/EmptyState'
import { Input } from '@/shared/components/ui/Input'
import { PageHeader } from '@/shared/components/ui/PageHeader'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/shared/components/ui/Table'
import {
  attachFileAssetOptions,
  cleanupFileAssetsOptions,
  deleteFileAssetOptions,
  downloadFileAssetOptions,
  fileAssetError,
  fileAssetKeys,
  fileAssetListOptions,
  uploadFileAssetOptions,
} from '../api/file-assets'

export function FileAssetPage({ token, canManage }: { token: string; canManage: boolean }) {
  const client = useQueryClient()
  const [page, setPage] = useState(1)
  const [file, setFile] = useState<File | null>(null)
  const [inputKey, setInputKey] = useState(0)
  const [target, setTarget] = useState<{ id: string; name: string; attached: boolean } | null>(null)
  const [notice, setNotice] = useState('')
  const [cleanupOpen, setCleanupOpen] = useState(false)
  const list = useQuery(fileAssetListOptions(token, page))
  const upload = useMutation({
    ...uploadFileAssetOptions(token),
    onSuccess: () => {
      setFile(null)
      setInputKey((value) => value + 1)
      setNotice('파일을 업로드했습니다.')
    },
    onSettled: () => client.invalidateQueries({ queryKey: fileAssetKeys.all }),
  })
  const remove = useMutation({
    ...deleteFileAssetOptions(token),
    onSuccess: async () => {
      setTarget(null)
      setNotice('파일을 삭제 대기 상태로 변경했습니다.')
      await client.invalidateQueries({ queryKey: fileAssetKeys.all })
    },
  })
  const attach = useMutation({
    ...attachFileAssetOptions(token),
    onSuccess: async () => {
      setNotice('파일을 연결 상태로 변경했습니다.')
      await client.invalidateQueries({ queryKey: fileAssetKeys.all })
    },
  })
  const download = useMutation(downloadFileAssetOptions(token))
  const cleanup = useMutation({
    ...cleanupFileAssetsOptions(token),
    onSuccess: () => {
      setCleanupOpen(false)
      setNotice('정리 요청을 접수했습니다. 작업은 백그라운드에서 진행됩니다.')
    },
  })
  const busy =
    upload.isPending ||
    remove.isPending ||
    attach.isPending ||
    download.isPending ||
    cleanup.isPending
  const error = upload.error ?? remove.error ?? attach.error ?? download.error

  return (
    <section className="mx-auto flex w-full max-w-6xl min-w-0 flex-col gap-6 px-4 py-6 sm:px-6 sm:py-8 lg:px-8">
      <PageHeader
        eyebrow="관리 / 파일 에셋"
        title="파일 에셋 관리"
        description="파일의 업로드와 상태를 관리합니다."
        actions={
          <>
            {canManage && (
              <Button
                variant="outline"
                disabled={busy}
                onClick={() => {
                  setNotice('')
                  cleanup.reset()
                  setCleanupOpen(true)
                }}
              >
                파일 정리
              </Button>
            )}
            <Button
              variant="outline"
              disabled={busy || list.isFetching}
              onClick={() => void list.refetch()}
            >
              새로고침
            </Button>
          </>
        }
      />
      {notice && (
        <Alert>
          <AlertDescription role="status">{notice}</AlertDescription>
        </Alert>
      )}
      {error && !(target && remove.error) && (
        <Alert variant="destructive">
          <AlertDescription>
            {fileAssetError(error)}
            {upload.error && ' 업로드가 실패한 경우 삭제 대기 전의 파일이 목록에 남을 수 있습니다.'}
          </AlertDescription>
        </Alert>
      )}
      {canManage && (
        <form
          aria-label="파일 에셋 업로드"
          className="flex flex-wrap items-end gap-3"
          onSubmit={(event) => {
            event.preventDefault()
            if (file) {
              setNotice('')
              upload.reset()
              remove.reset()
              attach.reset()
              download.reset()
              upload.mutate(file)
            }
          }}
        >
          <label className="grid min-w-48 flex-1 gap-2 text-sm font-medium">
            업로드할 파일
            <Input
              key={inputKey}
              type="file"
              disabled={busy}
              onChange={(event) => setFile(event.target.files?.[0] ?? null)}
            />
          </label>
          <Button type="submit" disabled={!file || busy}>
            {upload.isPending ? '업로드 중…' : '파일 업로드'}
          </Button>
        </form>
      )}
      <section aria-label="파일 에셋 목록" className="grid min-w-0 gap-4">
        <h2 className="text-lg font-semibold tracking-tight">파일 목록</h2>
        {list.isPending ? (
          <div
            role="status"
            className="rounded-lg border bg-card p-6 text-sm text-muted-foreground"
          >
            목록을 불러오는 중…
          </div>
        ) : list.isError ? (
          <div className="grid justify-items-start gap-3">
            <Alert variant="destructive">
              <AlertDescription>{fileAssetError(list.error)}</AlertDescription>
            </Alert>
            <Button variant="outline" onClick={() => void list.refetch()}>
              다시 시도
            </Button>
          </div>
        ) : list.data.items.length === 0 ? (
          <EmptyState>등록된 파일이 없습니다.</EmptyState>
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead scope="col">파일명</TableHead>
                <TableHead scope="col">상태</TableHead>
                <TableHead scope="col">크기</TableHead>
                <TableHead scope="col">업로더 ID</TableHead>
                <TableHead scope="col">등록일</TableHead>
                <TableHead scope="col" className="text-center">
                  작업
                </TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {list.data.items.map((asset) => (
                <TableRow key={asset.id} data-testid={`file-asset-row-${asset.id}`}>
                  <TableCell className="max-w-80 break-all whitespace-normal">
                    {asset.original_filename}
                  </TableCell>
                  <TableCell>
                    {asset.status === 'pending'
                      ? '연결 대기'
                      : asset.status === 'attached'
                        ? '연결됨'
                        : '삭제 대기'}
                  </TableCell>
                  <TableCell>{asset.size_bytes.toLocaleString('ko-KR')} B</TableCell>
                  <TableCell>{asset.uploaded_by}</TableCell>
                  <TableCell>{new Date(asset.created_at).toLocaleString('ko-KR')}</TableCell>
                  <TableCell className="w-px">
                    <div className="flex min-h-8 min-w-80 items-center justify-center gap-2">
                      {asset.status !== 'deleted' && (
                        <Button
                          size="sm"
                          variant="outline"
                          disabled={busy}
                          aria-label={`${asset.original_filename} 다운로드`}
                          onClick={() => {
                            setNotice('')
                            upload.reset()
                            remove.reset()
                            attach.reset()
                            download.mutate({ id: asset.id, filename: asset.original_filename })
                          }}
                        >
                          다운로드
                        </Button>
                      )}
                      {canManage && asset.status === 'pending' && (
                        <Button
                          size="sm"
                          variant="outline"
                          disabled={busy}
                          aria-label={`${asset.original_filename} 연결 상태로 변경`}
                          onClick={() => {
                            setNotice('')
                            upload.reset()
                            remove.reset()
                            attach.reset()
                            download.reset()
                            attach.mutate(asset.id)
                          }}
                        >
                          연결 상태로 변경
                        </Button>
                      )}
                      {canManage && asset.status !== 'deleted' && (
                        <Button
                          size="sm"
                          variant="destructive"
                          disabled={busy}
                          aria-label={`${asset.original_filename} 삭제`}
                          onClick={() => {
                            setNotice('')
                            upload.reset()
                            remove.reset()
                            attach.reset()
                            download.reset()
                            setTarget({
                              id: asset.id,
                              name: asset.original_filename,
                              attached: asset.status === 'attached',
                            })
                          }}
                        >
                          삭제
                        </Button>
                      )}
                    </div>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </section>
      <nav aria-label="파일 에셋 목록 페이지" className="flex items-center justify-center gap-3">
        <Button
          variant="outline"
          disabled={page === 1 || list.isFetching || busy}
          onClick={() => {
            setPage(page - 1)
            setTarget(null)
          }}
        >
          이전
        </Button>
        <span className="text-sm" aria-live="polite">
          {page} 페이지
        </span>
        <Button
          variant="outline"
          disabled={!list.data?.has_next || list.isFetching || busy}
          onClick={() => {
            setPage(page + 1)
            setTarget(null)
          }}
        >
          다음
        </Button>
      </nav>
      {target && (
        <DeleteDialog
          name={target.name}
          attached={target.attached}
          busy={busy}
          error={remove.error}
          onClose={() => {
            remove.reset()
            setTarget(null)
          }}
          onDelete={() => remove.mutate(target.id)}
        />
      )}
      {cleanupOpen && (
        <CleanupDialog
          busy={busy}
          error={cleanup.error}
          onClose={() => {
            cleanup.reset()
            setCleanupOpen(false)
          }}
          onConfirm={() => cleanup.mutate()}
        />
      )}
    </section>
  )
}

function CleanupDialog({
  busy,
  error,
  onClose,
  onConfirm,
}: {
  busy: boolean
  error: Error | null
  onClose: () => void
  onConfirm: () => void
}) {
  return (
    <Dialog
      labelledBy="file-asset-cleanup-title"
      onClose={onClose}
      busy={busy}
      className="fixed inset-0 m-auto w-[calc(100%-2rem)] max-w-lg rounded-xl border bg-card p-6 text-card-foreground shadow-xl backdrop:bg-black/50"
    >
      <div className="grid gap-4">
        <h2 id="file-asset-cleanup-title" className="text-lg font-semibold">
          파일 정리 확인
        </h2>
        <p className="text-sm">유예기간이 지난 미연결 파일과 삭제 대기 파일을 정리합니다.</p>
        {error && (
          <Alert variant="destructive">
            <AlertDescription>{fileAssetError(error)}</AlertDescription>
          </Alert>
        )}
        <div className="flex justify-end gap-2">
          <Button variant="outline" disabled={busy} onClick={onClose}>
            취소
          </Button>
          <Button variant="destructive" disabled={busy} onClick={onConfirm}>
            정리 요청
          </Button>
        </div>
      </div>
    </Dialog>
  )
}

function DeleteDialog({
  name,
  attached,
  busy,
  error,
  onClose,
  onDelete,
}: {
  name: string
  attached: boolean
  busy: boolean
  error: Error | null
  onClose: () => void
  onDelete: () => void
}) {
  return (
    <Dialog
      labelledBy="file-asset-delete-title"
      onClose={onClose}
      busy={busy}
      className="fixed inset-0 m-auto w-[calc(100%-2rem)] max-w-lg rounded-xl border bg-card p-6 text-card-foreground shadow-xl backdrop:bg-black/50"
    >
      <div className="grid gap-4">
        <h2 id="file-asset-delete-title" className="text-lg font-semibold">
          파일 삭제 확인
        </h2>
        <p className="break-all text-sm">{name} 파일을 삭제하시겠습니까?</p>
        <p className="text-sm text-muted-foreground">파일이 삭제 대기 상태로 변경됩니다.</p>
        {attached && (
          <p className="text-sm text-muted-foreground">
            연결된 곳에서 이 파일을 다운로드할 수 없게 됩니다.
          </p>
        )}
        {error && (
          <Alert variant="destructive">
            <AlertDescription>{fileAssetError(error)}</AlertDescription>
          </Alert>
        )}
        <div className="flex justify-end gap-2">
          <Button variant="outline" disabled={busy} onClick={onClose}>
            취소
          </Button>
          <Button variant="destructive" disabled={busy} onClick={onDelete}>
            삭제 확인
          </Button>
        </div>
      </div>
    </Dialog>
  )
}
