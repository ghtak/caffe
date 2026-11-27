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
              author_nickname: '작성자닉네임',
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

test('게시글 상세 주소를 새로고침하고 목록으로 돌아간다', async ({ page }) => {
  await page.route('**/api/auth/web/refresh', (route) => route.fulfill({ status: 401 }))
  await page.route('**/api/board/posts/1', (route) =>
    route.fulfill({
      json: {
        code: 'OK',
        data: {
          id: 1,
          title: '상세 글',
          board_slug: 'free',
          author_id: 1,
          author_nickname: '작성자',
          view_count: 0,
          comment_count: 0,
          created_at: '2026-01-01T00:00:00Z',
          content: '# 원문\n본문',
        },
      },
    }),
  )
  await page.route('**/api/board/posts?**', (route) =>
    route.fulfill({
      json: { code: 'OK', data: { items: [], page: 1, page_size: 20, has_next: false } },
    }),
  )
  await page.goto('/community/posts/1')
  await expect(page.getByRole('heading', { name: '상세 글' })).toBeVisible()
  await page.reload()
  // 상세 주소 복원 후 해당 게시판 목록으로 이동한다.
  await expect(page.getByRole('region', { name: '본문' })).toHaveText('# 원문\n본문')
  await page.getByRole('link', { name: '목록으로' }).click()
  await expect(page).toHaveURL(/\/community\/free$/)
  await expect(page.getByRole('heading', { name: '게시글 목록' })).toBeVisible()
})
