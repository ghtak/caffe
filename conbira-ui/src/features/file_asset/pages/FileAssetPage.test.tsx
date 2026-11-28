import { screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { http, HttpResponse } from 'msw'
import { describe, expect, it } from 'vitest'
import { server } from '../../../../test/msw/server'
import { renderWithProviders } from '../../../../test/renderWithProviders'

const item = {
  id: '00000000-0000-4000-8000-000000000001',
  status: 'pending',
  uploaded_by: 1,
  original_filename: 'report.txt',
  size_bytes: 5,
  content_type: 'text/plain',
  created_at: '2026-01-01T00:00:00Z',
  updated_at: '2026-01-01T00:00:00Z',
}
function session(permissions: string[]) {
  return http.get('*/api/auth/me', () =>
    HttpResponse.json({
      data: { exp: Math.floor(Date.now() / 1000) + 900, user: { permissions } },
    }),
  )
}
function list(items = [item]) {
  return HttpResponse.json({ code: 'OK', data: { items, page: 1, page_size: 20, has_next: false } })
}

describe('contract', () => {
  it('관리자가 정리를 확인하면 백그라운드 요청을 접수한다', async () => {
    let requested = false
    server.use(
      session(['file_asset.list.read', 'file_asset.manage']),
      http.get('*/api/file-assets', () => list()),
      http.post('*/api/file-assets/cleanup', ({ request }) => {
        expect(request.headers.get('authorization')).toBe('Bearer token')
        requested = true
        return HttpResponse.json({ code: 'OK' }, { status: 202 })
      }),
    )
    renderWithProviders('/admin/file-assets', 'token')
    const user = userEvent.setup()
    await user.click(await screen.findByRole('button', { name: '파일 정리' }))
    expect(requested).toBe(false)
    const dialog = screen.getByRole('dialog', { name: '파일 정리 확인' })
    await user.click(within(dialog).getByRole('button', { name: '정리 요청' }))
    expect(
      await screen.findByText('정리 요청을 접수했습니다. 작업은 백그라운드에서 진행됩니다.'),
    ).toBeVisible()
    expect(requested).toBe(true)
  })
  it('연결된 파일도 삭제할 수 있고 영향 범위를 안내한다', async () => {
    let deleted = false
    server.use(
      session(['file_asset.list.read', 'file_asset.manage']),
      http.get('*/api/file-assets', () =>
        list([{ ...item, status: deleted ? 'deleted' : 'attached' }]),
      ),
      http.delete(`*/api/file-assets/${item.id}`, () => {
        deleted = true
        return HttpResponse.json({ code: 'OK' })
      }),
    )
    renderWithProviders('/admin/file-assets', 'token')
    const user = userEvent.setup()
    await user.click(await screen.findByRole('button', { name: 'report.txt 삭제' }))
    const dialog = screen.getByRole('dialog', { name: '파일 삭제 확인' })
    expect(
      within(dialog).getByText('연결된 곳에서 이 파일을 다운로드할 수 없게 됩니다.'),
    ).toBeVisible()
    await user.click(within(dialog).getByRole('button', { name: '삭제 확인' }))
    expect(await screen.findByText('삭제 대기')).toBeVisible()
    expect(screen.queryByRole('button', { name: 'report.txt 삭제' })).not.toBeInTheDocument()
  })
  it('연결 대기 파일의 상태를 연결됨으로 변경한다', async () => {
    let attached = false
    server.use(
      session(['file_asset.list.read', 'file_asset.manage']),
      http.get('*/api/file-assets', () =>
        list([{ ...item, status: attached ? 'attached' : 'pending' }]),
      ),
      http.post(`*/api/file-assets/${item.id}/attach`, () => {
        attached = true
        return HttpResponse.json({ code: 'OK' })
      }),
    )
    renderWithProviders('/admin/file-assets', 'token')
    const user = userEvent.setup()
    await user.click(await screen.findByRole('button', { name: 'report.txt 연결 상태로 변경' }))
    expect(await screen.findByText('연결됨')).toBeVisible()
    expect(
      screen.queryByRole('button', { name: 'report.txt 연결 상태로 변경' }),
    ).not.toBeInTheDocument()
  })
  it('읽기 권한으로 다운로드를 표시하고 관리 작업은 숨긴다', async () => {
    server.use(
      session(['file_asset.list.read']),
      http.get('*/api/file-assets', () => list()),
    )
    renderWithProviders('/admin/file-assets', 'token')
    expect(await screen.findByRole('button', { name: 'report.txt 다운로드' })).toBeVisible()
    expect(screen.queryByRole('button', { name: '파일 업로드' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: '파일 정리' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'report.txt 삭제' })).not.toBeInTheDocument()
  })
  it('발급받은 URL에 파일을 업로드하고 목록을 갱신한다', async () => {
    let uploaded = false
    server.use(
      session(['file_asset.list.read', 'file_asset.manage']),
      http.get('*/api/file-assets', () => list(uploaded ? [item] : [])),
      http.post('*/api/file-assets', async ({ request }) => {
        expect(await request.json()).toEqual({
          original_filename: 'report.txt',
          size_bytes: 5,
          content_type: 'text/plain',
        })
        return HttpResponse.json({
          code: 'OK',
          data: {
            id: item.id,
            upload_url: 'https://storage.example/upload',
            headers: [['content-type', 'text/plain']],
            expires_in_seconds: 600,
          },
        })
      }),
      http.put('https://storage.example/upload', async ({ request }) => {
        expect(request.headers.get('content-type')).toBe('text/plain')
        uploaded = true
        return new HttpResponse(null, { status: 200 })
      }),
    )
    renderWithProviders('/admin/file-assets', 'token')
    await screen.findByText('등록된 파일이 없습니다.')
    const user = userEvent.setup()
    await user.upload(
      screen.getByLabelText('업로드할 파일'),
      new File(['hello'], 'report.txt', { type: 'text/plain' }),
    )
    await user.click(screen.getByRole('button', { name: '파일 업로드' }))
    expect(await screen.findByText('파일을 업로드했습니다.')).toBeVisible()
    expect(await screen.findByText('report.txt')).toBeVisible()
  })
  it('삭제 확인 후 삭제 대기 상태를 표시한다', async () => {
    let deleted = false
    server.use(
      session(['file_asset.list.read', 'file_asset.manage']),
      http.get('*/api/file-assets', () =>
        list([{ ...item, status: deleted ? 'deleted' : 'pending' }]),
      ),
      http.delete(`*/api/file-assets/${item.id}`, () => {
        deleted = true
        return HttpResponse.json({ code: 'OK' })
      }),
    )
    renderWithProviders('/admin/file-assets', 'token')
    const user = userEvent.setup()
    await user.click(await screen.findByRole('button', { name: 'report.txt 삭제' }))
    const dialog = screen.getByRole('dialog', { name: '파일 삭제 확인' })
    expect(deleted).toBe(false)
    await user.click(within(dialog).getByRole('button', { name: '삭제 확인' }))
    expect(await screen.findByText('삭제 대기')).toBeVisible()
    expect(screen.queryByRole('button', { name: 'report.txt 다운로드' })).not.toBeInTheDocument()
  })
})
