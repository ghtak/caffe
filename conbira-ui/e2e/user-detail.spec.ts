import { expect, test } from '@playwright/test'

test('사용자 상세 링크·새로고침·뒤로가기를 지원한다', async ({ page }) => {
  const errors: string[] = []
  page.on('pageerror', (error) => errors.push(error.message))
  const data = {
    id: 1,
    nickname: '상세 사용자',
    emails: ['user@example.test'],
    status: 'active',
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-02-01T00:00:00Z',
  }
  await page.addInitScript(() => sessionStorage.setItem('conbira.access-token', 'test-token'))
  await page.route('**/api/auth/web/refresh', (route) =>
    route.fulfill({
      json: {
        code: 'OK',
        data: {
          access_token: 'test-token',
          expires_at: Math.floor(Date.now() / 1000) + 900,
          claims: { permissions: ['auth.user.read'] },
        },
      },
    }),
  )
  await page.route('**/api/auth/me', (route) =>
    route.fulfill({
      json: {
        code: 'OK',
        data: {
          exp: Math.floor(Date.now() / 1000) + 900,
          user: { permissions: ['auth.user.read'] },
        },
      },
    }),
  )
  await page.route('**/api/auth/users?*', (route) =>
    route.fulfill({
      json: { code: 'OK', data: { items: [data], page: 1, page_size: 20, has_next: false } },
    }),
  )
  await page.route('**/api/auth/users/1', (route) => route.fulfill({ json: { code: 'OK', data } }))
  await page.goto('/admin/users')
  await page.getByRole('link', { name: '상세 사용자', exact: true }).click()
  // 상세 URL은 직접 접근·새로고침에도 같은 사용자를 조회한다.
  await expect(page).toHaveURL('/admin/users/1')
  await expect(page.getByRole('heading', { name: '사용자 상세' })).toBeVisible()
  await expect(page.getByText('user@example.test', { exact: true })).toBeVisible()
  await page.reload()
  await expect(page.getByText('상세 사용자', { exact: true })).toBeVisible()
  await page.screenshot({ path: 'test-results/user-detail.png', fullPage: true })
  await page.goBack()
  await expect(page).toHaveURL('/admin/users')
  await expect(page.getByRole('heading', { name: '사용자 목록' })).toBeVisible()
  expect(errors).toEqual([])
})
