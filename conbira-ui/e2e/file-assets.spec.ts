import { expect, test } from '@playwright/test'

test('파일 에셋을 presigned URL로 업로드하고 다운로드한다', async ({ page }) => {
  const name = `e2e-${crypto.randomUUID()}.txt`
  const id = '00000000-0000-4000-8000-000000000001'
  const item = {
    id,
    status: 'pending',
    uploaded_by: 1,
    original_filename: name,
    size_bytes: 5,
    content_type: 'text/plain',
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-01-01T00:00:00Z',
  }
  let uploaded = false
  const errors: string[] = []
  page.on('pageerror', (error) => errors.push(error.message))
  await page.route('**/api/**', async (route) => {
    const request = route.request()
    const url = new URL(request.url())
    if (!url.pathname.startsWith('/api/')) return route.continue()
    const claims = { permissions: ['file_asset.list.read', 'file_asset.manage'] }
    if (url.pathname === '/api/auth/web/refresh') {
      await route.fulfill({
        json: {
          code: 'OK',
          data: { access_token: 'token', expires_at: Math.floor(Date.now() / 1000) + 900, claims },
        },
      })
    } else if (url.pathname === '/api/auth/me') {
      await route.fulfill({
        json: { data: { user: claims, exp: Math.floor(Date.now() / 1000) + 900 } },
      })
    } else if (url.pathname === '/api/file-assets' && request.method() === 'POST') {
      expect(request.postDataJSON()).toEqual({
        original_filename: name,
        size_bytes: 5,
        content_type: 'text/plain',
      })
      await route.fulfill({
        json: {
          code: 'OK',
          data: {
            id,
            upload_url: new URL('/storage-object', url).href,
            headers: [['content-type', 'text/plain']],
            expires_in_seconds: 600,
          },
        },
      })
    } else if (url.pathname === '/api/file-assets') {
      await route.fulfill({
        json: {
          code: 'OK',
          data: { items: uploaded ? [item] : [], page: 1, page_size: 20, has_next: false },
        },
      })
    } else if (url.pathname === `/api/file-assets/${id}/download`) {
      await route.fulfill({
        json: {
          code: 'OK',
          data: {
            download_url: new URL('/storage-object', url).href,
            headers: [],
            expires_in_seconds: 180,
          },
        },
      })
    } else await route.fulfill({ status: 404 })
  })
  await page.route('**/storage-object', async (route) => {
    if (route.request().method() === 'PUT') {
      expect(route.request().postData()).toBe('hello')
      expect(route.request().headers()['content-type']).toBe('text/plain')
      uploaded = true
      await route.fulfill({ status: 200 })
    } else
      await route.fulfill({
        body: 'hello',
        contentType: 'text/plain',
        headers: { 'content-disposition': `attachment; filename="${name}"` },
      })
  })
  await page.goto('/admin')
  await page.getByRole('link', { name: '파일 에셋 관리', exact: true }).click()
  await expect(page.getByText('등록된 파일이 없습니다.')).toBeVisible()
  await page
    .getByLabel('업로드할 파일')
    .setInputFiles({ name, mimeType: 'text/plain', buffer: Buffer.from('hello') })
  await page.getByRole('button', { name: '파일 업로드' }).click()
  await expect(page.getByText(name, { exact: true })).toBeVisible()
  const downloading = page.waitForEvent('download')
  await page.getByRole('button', { name: `${name} 다운로드` }).click()
  const download = await downloading
  expect(download.suggestedFilename()).toBe(name)
  expect(await download.failure()).toBeNull()
  expect(errors).toEqual([])
})
