import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import { isHTTPError } from 'ky'
import { useEffect, useRef, useState } from 'react'
import { Button } from '@/shared/components/ui/Button'
import { Input } from '@/shared/components/ui/Input'
import { candidateOptions, permissionsOptions, roleChangeOptions, roleDetailOptions, roleKeys, rolePermissionsOptions, roleUsersOptions } from '../api/role-detail'

function message(error: unknown) {
  if (isHTTPError(error) && error.response.status === 403) return '이 작업을 수행할 권한이 없습니다.'
  if (isHTTPError(error) && error.response.status === 404) return '대상을 찾을 수 없습니다.'
  if (isHTTPError(error) && error.response.status === 400) return '이미 할당된 항목입니다.'
  return '요청을 처리하지 못했습니다. 다시 시도해 주세요.'
}
function PermissionPanel({ id, token, canEdit }: { id: number; token: string; canEdit: boolean }) {
  const client = useQueryClient()
  const all = useQuery(permissionsOptions(token))
  const assigned = useQuery(rolePermissionsOptions(id, token))
  const change = useMutation({ ...roleChangeOptions(token), onSuccess: () => client.invalidateQueries({ queryKey: roleKeys.detail(id) }) })
  if (all.isPending || assigned.isPending) return <p role="status">권한을 불러오는 중…</p>
  if (all.isError || assigned.isError) return <p role="alert">{message(all.error ?? assigned.error)}</p>
  const sortedPermissions = [...all.data].sort((a, b) => {
    const assignedA = assigned.data.some((item) => item.id === a.id)
    const assignedB = assigned.data.some((item) => item.id === b.id)
    return Number(assignedA) - Number(assignedB) || a.name.localeCompare(b.name, 'ko') || a.id - b.id
  })
  return <div className="grid content-start gap-4"><p>권한 변경은 사용자가 다시 로그인할 때 적용됩니다.</p>{change.isError && <p role="alert">{message(change.error)}</p>}{all.data.length === 0 && <p>등록된 권한이 없습니다.</p>}<ul className="grid gap-2">{sortedPermissions.map((permission) => {
    const selected = assigned.data.some((item) => item.id === permission.id)
    return <li key={permission.id} className={`flex items-center justify-between gap-3 rounded-lg border p-4 ${selected ? 'border-primary/40 bg-primary/5' : 'bg-card'}`}><div className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1"><span className="font-medium">{permission.name}</span><code className="break-all text-xs text-muted-foreground">{permission.code}</code><span className="text-sm text-muted-foreground">{permission.description}</span><span className="text-xs text-muted-foreground">{selected ? '할당됨' : '미할당'}</span></div>{canEdit ? <Button type="button" variant={selected ? "outline" : "default"} className={selected ? "border-destructive/40 text-destructive hover:text-destructive" : undefined} aria-label={`${permission.name} ${selected ? '해제' : '추가'}`} disabled={change.isPending} onClick={() => change.mutate({ path: `/api/rbac/roles/${id}/permissions/${permission.id}`, add: !selected })}>{selected ? '해제' : '추가'}</Button> : <span>{selected ? '할당됨' : '미할당'}</span>}</li>
  })}</ul></div>
}
type Candidate = { id: number; nickname: string; emails: string[]; assigned: boolean }

function CandidateRow({ user, pending, onAdd }: { user: Candidate; pending: boolean; onAdd: (id: number) => void }) {
  return <li className="flex items-center justify-between gap-3 border-b py-3 last:border-0">
    <div className="min-w-0"><p className="font-medium">{user.nickname}</p><p className="break-all text-sm text-muted-foreground">{user.emails.join(', ') || '이메일 없음'}</p></div>
    <Button type="button" size="sm" variant={user.assigned ? 'secondary' : 'default'} aria-label={`${user.nickname} ${user.assigned ? '할당됨' : '추가'}`} disabled={pending || user.assigned} onClick={() => onAdd(user.id)}>{user.assigned ? '할당됨' : '추가'}</Button>
  </li>
}

function AddUserDialog({ roleId, token, onClose, onAdded }: { roleId: number; token: string; onClose: () => void; onAdded: () => void }) {
  const dialog = useRef<HTMLDialogElement>(null)
  const client = useQueryClient()
  const [page, setPage] = useState(1)
  const [name, setName] = useState('')
  const [search, setSearch] = useState('')
  const candidates = useQuery(candidateOptions(roleId, token, search, page))
  const change = useMutation({ ...roleChangeOptions(token), onSuccess: async () => {
    await client.invalidateQueries({ queryKey: roleKeys.detail(roleId) })
    onAdded()
  } })
  useEffect(() => {
    const element = dialog.current
    if (!element) return
    if (typeof element.showModal === 'function') element.showModal()
    else element.setAttribute('open', '')
    return () => { if (element.open && typeof element.close === 'function') element.close() }
  }, [])
  return <dialog ref={dialog} aria-labelledby="add-user-title" onCancel={(event) => { event.preventDefault(); onClose() }} className="fixed inset-0 m-auto max-h-[min(90vh,42rem)] w-[calc(100%-2rem)] max-w-xl overflow-hidden rounded-xl border bg-card p-0 text-card-foreground shadow-xl backdrop:bg-black/50">
    <div className="flex max-h-[min(90vh,42rem)] flex-col">
      <header className="flex items-start justify-between gap-4 border-b p-5"><div><h2 id="add-user-title" className="text-lg font-semibold">사용자 추가</h2><p className="mt-1 text-sm text-muted-foreground">이 역할에 추가할 사용자를 검색하세요. 변경은 사용자가 다시 로그인할 때 적용됩니다.</p></div><Button type="button" variant="ghost" size="sm" onClick={onClose} aria-label="사용자 추가 닫기">닫기</Button></header>
      <div className="grid content-start gap-3 overflow-y-auto p-5"><form className="flex gap-2" onSubmit={(event) => { event.preventDefault(); setSearch(name.trim()); setPage(1) }}><Input aria-label="사용자 이름 검색" placeholder="사용자 이름" value={name} onChange={(event) => setName(event.target.value)} /><Button type="submit" className="h-9">검색</Button></form>
        {change.isError && <p role="alert" className="text-sm text-destructive">{message(change.error)}</p>}
        {candidates.isPending ? <p role="status">사용자를 불러오는 중…</p> : candidates.isError ? <p role="alert">{message(candidates.error)}</p> : <><ul className="rounded-lg border px-3">{candidates.data.items.map((user) => <CandidateRow key={user.id} user={user} pending={change.isPending} onAdd={(userId) => change.mutate({ path: `/api/rbac/users/${userId}/roles/${roleId}`, add: true })} />)}</ul>{candidates.data.items.length === 0 && <p>검색된 사용자가 없습니다.</p>}<nav aria-label="사용자 검색 페이지" className="flex items-center justify-center gap-3"><Button type="button" variant="outline" size="sm" disabled={page === 1 || candidates.isFetching} onClick={() => setPage(page - 1)}>이전</Button><span className="text-sm">{page}</span><Button type="button" variant="outline" size="sm" disabled={!candidates.data.has_next || candidates.isFetching} onClick={() => setPage(page + 1)}>다음</Button></nav></>}
      </div>
    </div>
  </dialog>
}

function UserPanel({ id, token, canEdit }: { id: number; token: string; canEdit: boolean }) {
  const client = useQueryClient()
  const [page, setPage] = useState(1)
  const [adding, setAdding] = useState(false)
  const [notice, setNotice] = useState('')
  const addButton = useRef<HTMLButtonElement>(null)
  const users = useQuery(roleUsersOptions(id, token, page))
  const change = useMutation({ ...roleChangeOptions(token), onSuccess: async () => {
    await client.invalidateQueries({ queryKey: roleKeys.detail(id) })
    setNotice('사용자의 역할을 해제했습니다.')
  } })
  function closeDialog() { setAdding(false); addButton.current?.focus() }
  return <div className="grid content-start gap-4"><div className="flex flex-wrap items-center justify-between gap-3"><p className="text-sm text-muted-foreground">사용자 역할 변경은 해당 사용자가 다시 로그인할 때 적용됩니다.</p>{canEdit && <Button ref={addButton} type="button" onClick={() => setAdding(true)}>+ 사용자 추가</Button>}</div>
    <p aria-live="polite" className="text-sm text-muted-foreground">{notice}</p>{change.isError && <p role="alert" className="text-destructive">{message(change.error)}</p>}
    {adding && <AddUserDialog roleId={id} token={token} onClose={closeDialog} onAdded={() => setNotice('사용자를 역할에 추가했습니다.')} />}
    {users.isPending ? <p role="status">사용자를 불러오는 중…</p> : users.isError ? <p role="alert">{message(users.error)}</p> : <><ul className="rounded-lg border px-4">{users.data.items.map((user) => <li className="flex items-center justify-between gap-3 border-b py-3 last:border-0" key={user.id}><div className="min-w-0"><p className="font-medium">{user.nickname}</p><p className="break-all text-sm text-muted-foreground">{user.emails.join(', ') || '이메일 없음'}</p></div>{canEdit && <Button type="button" variant="outline" size="sm" className="border-destructive/40 text-destructive hover:text-destructive" aria-label={`${user.nickname} 해제`} disabled={change.isPending} onClick={() => change.mutate({ path: `/api/rbac/users/${user.id}/roles/${id}`, add: false })}>역할 해제</Button>}</li>)}</ul>{users.data.items.length === 0 && <p>할당된 사용자가 없습니다.</p>}<nav aria-label="할당된 사용자 페이지" className="flex items-center justify-center gap-3"><Button type="button" variant="outline" size="sm" disabled={page === 1 || users.isFetching} onClick={() => setPage(page - 1)}>이전</Button><span className="text-sm">{page}</span><Button type="button" variant="outline" size="sm" disabled={!users.data.has_next || users.isFetching} onClick={() => setPage(page + 1)}>다음</Button></nav></>}
  </div>
}
export function RoleDetailPage({ id, token, permissions }: { id: number; token: string; permissions: string[] }) {
  const [tab, setTab] = useState<'permissions' | 'users'>('permissions')
  const role = useQuery(roleDetailOptions(id, token))
  if (role.isPending) return <p role="status" className="p-8">역할을 불러오는 중…</p>
  if (role.isError) return <p role="alert" className="p-8">{message(role.error)}</p>
  return <section className="mx-auto grid w-full max-w-7xl min-w-0 content-start gap-8 p-5 sm:p-8 lg:p-12"><header className="grid gap-2"><Link to="/admin/roles" className="text-sm text-primary">역할 관리</Link><h1 className="text-3xl font-bold">{role.data.name}</h1><code>{role.data.code}</code><p className="text-muted-foreground">{role.data.description}</p></header><div className="grid content-start gap-5 rounded-lg border bg-card p-5"><nav aria-label="역할 설정" className="flex gap-2 border-b pb-4"><Button type="button" variant={tab === 'permissions' ? 'default' : 'outline'} aria-current={tab === 'permissions' ? 'page' : undefined} onClick={() => setTab('permissions')}>권한 설정</Button><Button type="button" variant={tab === 'users' ? 'default' : 'outline'} aria-current={tab === 'users' ? 'page' : undefined} onClick={() => setTab('users')}>사용자 설정</Button></nav>{tab === 'permissions' ? <PermissionPanel id={id} token={token} canEdit={permissions.includes('rbac.role.manage')} /> : <UserPanel id={id} token={token} canEdit={permissions.includes('rbac.user_role.manage')} />}</div></section>
}
