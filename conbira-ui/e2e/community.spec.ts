import { expect, test } from '@playwright/test'

test('비로그인 커뮤니티에서 게시판 이동과 새로고침을 지원한다', async ({ page }) => {
  const errors: string[] = []
  page.on('pageerror', (error) => errors.push(error.message))
  await page.route('**/api/auth/web/refresh', (route) => route.fulfill({ status: 401 }))
  await page.route('**/api/board/posts?**', (route) => {
    const board = new URL(route.request().url()).searchParams.get('board_slug')
    return route.fulfill({
      json: {
        code: 'OK',
        data: {
          items: [
            {
              id: 1,
              title: `${board} 게시글`,
              board_slug: board,
              author_id: 1,
              view_count: 0,
              comment_count: 0,
              created_at: '2026-01-01T00:00:00Z',
            },
          ],
          page: 1,
          page_size: 20,
          has_next: false,
        },
      },
    })
  })
  await page.goto('/')
  await page.getByRole('link', { name: '커뮤니티' }).click()
  await expect(page.getByText('notice 게시글', { exact: true })).toBeVisible()
  await page.getByRole('link', { name: '자유게시판', exact: true }).click()
  await expect(page.getByText('free 게시글', { exact: true })).toBeVisible()
  await page.reload()
  // 인증 없이 게시판 주소를 복원하고 이전 게시판으로 돌아간다.
  await expect(page.getByText('free 게시글', { exact: true })).toBeVisible()
  await page.goBack()
  await expect(page.getByText('notice 게시글', { exact: true })).toBeVisible()
  expect(errors).toEqual([])
})
