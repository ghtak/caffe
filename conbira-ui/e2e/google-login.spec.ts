import { expect, test } from '@playwright/test'

test.beforeEach(async ({ page }) => {
  await page.route('**/api/auth/web/refresh', (route) => route.fulfill({ status: 401 }))
})

test.describe('regression', () => {
  test('캐시 복원 이벤트 후 Google 로그인을 다시 시작한다', async ({ page }) => {
    let starts = 0
    await page.route('**/api/auth/login/google', async (route) => {
      starts++
      if (starts === 1) return
      return route.fulfill({
        json: {
          data: {
            authorization_url: 'http://127.0.0.1:4173/fake-google',
            state: 'state',
            browser_proof: 'proof',
          },
        },
      })
    })
    await page.route('**/fake-google', (route) => route.fulfill({ status: 200, body: 'Google' }))

    await page.goto('/login')
    await page.getByRole('button', { name: 'Google로 로그인' }).click()
    await expect(page.getByRole('button', { name: 'Google 로그인 중…' })).toBeDisabled()
    await page.evaluate(() =>
      window.dispatchEvent(new PageTransitionEvent('pageshow', { persisted: true })),
    )
    // 기대 결과: 캐시 복원 이벤트 후 버튼과 요청 가드가 해제된다.
    await expect(page.getByRole('button', { name: 'Google로 로그인' })).toBeEnabled()
    await page.getByRole('button', { name: 'Google로 로그인' }).click()
    await expect(page).toHaveURL('/fake-google')
    expect(starts).toBe(2)
  })
})

test.describe('contract', () => {
  test('Google 로그인 시작과 콜백이 같은 탭에서 세션을 생성한다', async ({ page }) => {
    const errors: string[] = []
    page.on('pageerror', (error) => errors.push(error.message))
    await page.route('**/api/auth/login/google', async (route) => {
      await route.fulfill({
        json: {
          data: {
            authorization_url: 'http://127.0.0.1:4173/fake-google',
            state: 'tab-state',
            browser_proof: 'proof',
          },
        },
      })
    })
    await page.route('**/fake-google', (route) => route.fulfill({ status: 200, body: 'Google' }))
    await page.route('**/api/auth/web/login/google/complete', async (route) => {
      await route.fulfill({
        json: { data: { access_token: 'google-token' } },
      })
    })
    await page.route('**/api/auth/me', (route) =>
      route.fulfill({
        json: {
          data: { exp: Math.floor(Date.now() / 1000) + 900, user: { roles: ['user'] } },
        },
      }),
    )

    await page.goto('/login')
    await page.getByRole('button', { name: 'Google로 로그인' }).click()
    await expect(page).toHaveURL('/fake-google')
    // 같은 탭에 콜백 증명을 보관한다.
    expect(
      await page.evaluate(() =>
        JSON.parse(sessionStorage.getItem('conbira.google-login') ?? 'null'),
      ),
    ).toEqual({ state: 'tab-state', browser_proof: 'proof' })

    await page.goto('/login/google/callback?code=auth-code&state=tab-state')
    // 실제 브라우저 URL에서 자격증명을 제거하고 기존 세션으로 이동한다.
    await expect(page.getByRole('button', { name: '로그아웃' })).toBeVisible()
    await expect(page).toHaveURL('/')
    expect(await page.evaluate(() => sessionStorage.getItem('conbira.google-login'))).toBeNull()
    expect(errors).toEqual([])
  })
})
