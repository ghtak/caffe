import { expect, test } from '@playwright/test'

test.describe('contract', () => {
  test('파일 업로드 대화상자에서 전송한 실제 multipart 파일을 다운로드한다', async ({ page }) => {
    const name = `e2e-${crypto.randomUUID()}.txt`
    const secondName = `e2e-${crypto.randomUUID()}.txt`
    const groupId = '00000000-0000-4000-8000-000000000002'
    const file = {
      file_id: '00000000-0000-4000-8000-000000000001',
      group_id: groupId,
      status: 'pending',
      original_filename: name,
      size_bytes: 5,
      content_type: 'text/plain',
      created_at: '2026-01-01T00:00:00Z',
    }
    let uploaded = false
    const errors: string[] = []
    page.on('pageerror', (error) => errors.push(error.message))
    await page.route('**/api/**', async (route) => {
      const request = route.request()
      const path = new URL(request.url()).pathname
      if (!path.startsWith('/api/')) return route.continue()
      const claims = { permissions: ['file.list.read', 'file.manage'] }
      if (path === '/api/auth/web/refresh') {
        await route.fulfill({
          json: {
            code: 'OK',
            data: {
              access_token: 'file-token',
              expires_at: Math.floor(Date.now() / 1000) + 900,
              claims,
            },
          },
        })
      } else if (path === '/api/auth/me') {
        await route.fulfill({
          json: { data: { user: claims, exp: Math.floor(Date.now() / 1000) + 900 } },
        })
      } else if (path === '/api/files' && request.method() === 'POST') {
        const body = await new Request(request.url(), {
          method: 'POST',
          headers: await request.allHeaders(),
          body: new Uint8Array(request.postDataBuffer()!).buffer,
        }).formData()
        const files = body.getAll('file') as File[]
        // 실제 브라우저가 생성한 파일명·내용·그룹을 HTTP 경계에서 검증한다.
        expect(files.map((entry) => entry.name)).toEqual([name, secondName])
        expect(await files[0].text()).toBe('hello')
        expect(await files[1].text()).toBe('world')
        expect(body.get('group_id')).toBe(groupId)
        expect(request.headers().authorization).toBe('Bearer file-token')
        uploaded = true
        await route.fulfill({ status: 201, json: { code: 'OK', data: [file] } })
      } else if (path === '/api/files') {
        await route.fulfill({
          json: {
            code: 'OK',
            data: { items: uploaded ? [file] : [], page: 1, page_size: 20, has_next: false },
          },
        })
      } else if (path === `/api/files/${file.file_id}/download`) {
        await route.fulfill({
          json: {
            code: 'OK',
            data: {
              url: new URL('/test-file-download', request.url()).href,
              expires_at: '2026-11-26T23:59:00Z',
            },
          },
        })
      } else {
        await route.fulfill({ status: 404, json: { code: 'NOT_FOUND' } })
      }
    })
    await page.route('**/test-file-download', (route) =>
      route.fulfill({
        headers: {
          'Content-Type': 'text/plain',
          'Content-Disposition': `attachment; filename="${name}"`,
        },
        body: 'hello',
      }),
    )
    await page.goto('/admin')
    await page.getByRole('link', { name: '파일 관리', exact: true }).click()
    await expect(page).toHaveURL('/admin/files')
    await expect(page.getByText('등록된 파일이 없습니다.')).toBeVisible()
    await page.getByRole('button', { name: '파일 업로드' }).click()
    const dialog = page.getByRole('dialog', { name: '파일 업로드' })
    await expect(dialog).toBeVisible()
    await dialog.getByLabel('업로드할 파일').setInputFiles([
      { name, mimeType: 'text/plain', buffer: Buffer.from('hello') },
      { name: secondName, mimeType: 'text/plain', buffer: Buffer.from('world') },
    ])
    await dialog.getByLabel('그룹 ID (비워 두면 새 그룹)').fill(groupId)
    await dialog.getByRole('button', { name: /^업로드$/ }).click()
    await expect(dialog).not.toBeVisible()
    await expect(page.getByText(name, { exact: true })).toBeVisible()
    const downloading = page.waitForEvent('download')
    await page.getByRole('button', { name: `${name} 다운로드`, exact: true }).click()
    const download = await downloading
    expect(download.suggestedFilename()).toBe(name)
    expect(await download.failure()).toBeNull()
    await page.screenshot({ path: '/tmp/caffe-files-preview.png', fullPage: true })
    expect(errors).toEqual([])
  })
})
