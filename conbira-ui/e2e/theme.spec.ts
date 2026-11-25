import { expect, test } from '@playwright/test'

test.describe('contract', () => {
  test('시스템 테마를 따르고 수동 선택을 새로고침과 화면 이동 후에도 유지한다', async ({
    page,
  }) => {
    await page.route('**/api/auth/web/refresh', (route) => route.fulfill({ status: 401 }))
    await page.emulateMedia({ colorScheme: 'light' })
    await page.goto('/')
    const theme = page.getByRole('combobox', { name: '화면 테마' })
    const background = () => page.evaluate(() => getComputedStyle(document.body).backgroundColor)
    await expect(theme).toHaveValue('system')
    const lightBackground = await background()

    await page.emulateMedia({ colorScheme: 'dark' })
    // 시스템을 선택한 상태에서 운영체제 설정이 바뀌면 다크 배경을 적용한다.
    await expect.poll(background).not.toBe(lightBackground)
    const darkBackground = await background()

    await theme.selectOption('light')
    // 시스템이 다크여도 수동 라이트 선택을 우선 적용한다.
    await expect.poll(background).toBe(lightBackground)

    await theme.selectOption('dark')
    await page.emulateMedia({ colorScheme: 'light' })
    await page.reload()
    await page.getByRole('link', { name: '로그인' }).click()

    // 수동 다크 선택을 복원하고 로그인 화면에도 같은 배경을 적용한다.
    await expect(page).toHaveURL('/login')
    await expect(theme).toHaveValue('dark')
    await expect.poll(background).toBe(darkBackground)

    await theme.selectOption('system')
    // 시스템 설정으로 돌아가면 운영체제의 라이트 배경을 적용한다.
    await expect.poll(background).toBe(lightBackground)
  })
})
