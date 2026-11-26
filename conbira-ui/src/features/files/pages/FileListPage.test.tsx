import { screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { http, HttpResponse } from 'msw'
import { describe, expect, it } from 'vitest'
import { server } from '../../../../test/msw/server'
import { renderWithProviders } from '../../../../test/renderWithProviders'

const item = {
  file_id: '00000000-0000-4000-8000-000000000001',
  group_id: '00000000-0000-4000-8000-000000000002',
  status: 'pending',
  original_filename: 'report.txt',
  size_bytes: 5,
  content_type: 'text/plain',
  created_at: '2026-01-01T00:00:00Z',
}
function session(permissions = ['file.list.read', 'file.manage']) {
  return http.get('*/api/auth/me', () =>
    HttpResponse.json({
      data: { exp: Math.floor(Date.now() / 1000) + 900, user: { permissions } },
    }),
  )
}
function list(items = [item], page = 1, has_next = false) {
  return HttpResponse.json({ code: 'OK', data: { items, page, page_size: 20, has_next } })
}

describe('contract', () => {
  it('목록 권한만 있으면 파일을 조회하고 관리 버튼은 표시하지 않는다', async () => {
    server.use(
      session(['file.list.read']),
      http.get('*/api/files', () => list()),
    )
    renderWithProviders('/admin/files', 'file-token')
    // 읽기 전용 사용자는 파일 메타데이터만 확인한다.
    expect(await screen.findByText('report.txt')).toBeVisible()
    expect(screen.queryByRole('button', { name: '파일 업로드' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'report.txt 다운로드' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'report.txt 삭제' })).not.toBeInTheDocument()
  })
  it('파일명 검색과 페이지 이동을 전달하고 새 검색은 첫 페이지로 이동한다', async () => {
    server.use(
      session(['file.list.read']),
      http.get('*/api/files', ({ request }) => {
        const params = new URL(request.url).searchParams
        const name = params.get('name') ?? ''
        const page = Number(params.get('page'))
        expect(request.headers.get('Authorization')).toBe('Bearer file-token')
        return list(
          [{ ...item, original_filename: `${name || '전체'}-${page}.txt` }],
          page,
          page === 1,
        )
      }),
    )
    renderWithProviders('/admin/files', 'file-token')
    await screen.findByText('전체-1.txt')
    const user = userEvent.setup()
    await user.type(screen.getByLabelText('파일명'), 'report')
    await user.click(screen.getByRole('button', { name: '검색' }))
    expect(await screen.findByText('report-1.txt')).toBeVisible()
    await user.click(screen.getByRole('button', { name: '다음' }))
    expect(await screen.findByText('report-2.txt')).toBeVisible()
    expect(screen.getByRole('button', { name: '다음' })).toHaveAttribute('aria-disabled', 'true')
    await user.clear(screen.getByLabelText('파일명'))
    await user.type(screen.getByLabelText('파일명'), 'other')
    await user.click(screen.getByRole('button', { name: '검색' }))
    // 새 검색은 페이지를 초기화한다.
    expect(await screen.findByText('other-1.txt')).toBeVisible()
    expect(screen.getByText('1 페이지')).toBeVisible()
  })
  it('여러 파일을 하나의 multipart 요청으로 업로드하고 목록을 갱신한다', async () => {
    let uploaded = false
    server.use(
      session(),
      http.get('*/api/files', () => list(uploaded ? [item] : [])),
      http.post('*/api/files', ({ request }) => {
        expect(request.headers.get('content-type')).toContain('multipart/form-data')
        uploaded = true
        return HttpResponse.json({ code: 'OK', data: [item] }, { status: 201 })
      }),
    )
    renderWithProviders('/admin/files', 'file-token')
    await screen.findByText('등록된 파일이 없습니다.')
    const user = userEvent.setup()
    await user.click(screen.getByRole('button', { name: '파일 업로드' }))
    const dialog = screen.getByRole('dialog', { name: '파일 업로드' })
    await user.upload(within(dialog).getByLabelText('업로드할 파일'), [
      new File(['hello'], 'report.txt', { type: 'text/plain' }),
      new File(['second'], 'second.txt', { type: 'text/plain' }),
    ])
    await user.click(within(dialog).getByRole('button', { name: /^업로드$/ }))
    // 업로드 결과를 목록 재조회로 반영한다.
    expect(await screen.findByText('파일을 업로드했습니다.')).toBeVisible()
    expect(screen.queryByRole('dialog', { name: '파일 업로드' })).not.toBeInTheDocument()
    expect(await screen.findByText('report.txt')).toBeVisible()
  })
  it('선택한 파일을 리소스에 연결한 뒤 연결 상태를 갱신한다', async () => {
    let attached = false
    server.use(
      session(),
      http.get('*/api/files', () => list([{ ...item, status: attached ? 'attached' : 'pending' }])),
      http.post(`*/api/file-groups/${item.group_id}/attach`, async ({ request }) => {
        expect(await request.json()).toEqual({
          resource_type: 'post',
          resource_key: '42',
          attach_file_ids: [item.file_id],
          detach_file_ids: [],
        })
        attached = true
        return HttpResponse.json({ code: 'OK' })
      }),
    )
    renderWithProviders('/admin/files', 'file-token')
    const user = userEvent.setup()
    await user.click(await screen.findByRole('button', { name: 'report.txt 연결' }))
    await user.type(screen.getByLabelText('리소스 유형'), 'post')
    await user.type(screen.getByLabelText('리소스 키'), '42')
    await user.click(
      within(screen.getByRole('form', { name: '파일 연결' })).getByRole('button', {
        name: /^연결$/,
      }),
    )
    // 연결 완료 후 파일의 표시 상태가 바뀐다.
    expect(await screen.findByText('연결됨')).toBeVisible()
    expect(screen.queryByRole('button', { name: 'report.txt 연결' })).not.toBeInTheDocument()
  })
  it('삭제 확인 후 파일을 삭제하고 빈 목록을 표시한다', async () => {
    let deleted = false
    server.use(
      session(),
      http.get('*/api/files', () => list(deleted ? [] : [item])),
      http.delete(`*/api/files/${item.file_id}`, () => {
        deleted = true
        return HttpResponse.json({ code: 'OK' })
      }),
    )
    renderWithProviders('/admin/files', 'file-token')
    const user = userEvent.setup()
    await user.click(await screen.findByRole('button', { name: 'report.txt 삭제' }))
    expect(deleted).toBe(false)
    await user.click(screen.getByRole('button', { name: '삭제 확인' }))
    // 명시적으로 삭제를 확인한 다음 목록에서 제거한다.
    expect(await screen.findByText('파일을 삭제했습니다.')).toBeVisible()
    expect(await screen.findByText('등록된 파일이 없습니다.')).toBeVisible()
  })
})

describe('edge', () => {
  it('관리 권한만으로 목록에 접근하거나 목록 요청을 보낼 수 없다', async () => {
    let requested = false
    server.use(
      session(['file.manage', 'rbac.read']),
      http.get('*/api/files', () => {
        requested = true
        return list()
      }),
    )
    renderWithProviders('/admin/files', 'file-token')
    // 목록 권한과 관리 권한은 독립적으로 적용한다.
    expect(await screen.findByRole('alert')).toHaveTextContent(
      '파일 목록을 조회할 권한이 없습니다.',
    )
    expect(requested).toBe(false)
  })
  it('목록 오류에서 재시도하고 빈 검색 결과를 안내한다', async () => {
    server.use(
      session(),
      http.get('*/api/files', () => HttpResponse.json({ code: 'BUSY' }, { status: 503 })),
    )
    renderWithProviders('/admin/files', 'file-token')
    expect(await screen.findByRole('alert')).toHaveTextContent('요청을 처리하지 못했습니다.')
    server.use(http.get('*/api/files', () => list([])))
    const user = userEvent.setup()
    await user.click(screen.getByRole('button', { name: '다시 시도' }))
    expect(await screen.findByText('등록된 파일이 없습니다.')).toBeVisible()
    await user.type(screen.getByLabelText('파일명'), 'missing')
    await user.click(screen.getByRole('button', { name: '검색' }))
    expect(await screen.findByText('검색 조건에 맞는 파일이 없습니다.')).toBeVisible()
  })
})
