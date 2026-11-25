import { expect, test } from '@playwright/test'
import { http, HttpResponse } from 'msw'
import { server } from '../test/msw/server'

test.beforeAll(() => server.listen({ onUnhandledFrame: 'error' }))
test.beforeEach(async ({ page }) => {
  // 브라우저의 API 요청을 Node의 MSW 핸들러에 전달한다.
  await page.context().route('**/api/**', async (route) => {
    const request = route.request()
    const response = await fetch(request.url(), {
      method: request.method(),
      headers: await request.allHeaders(),
      body: request.postData() ?? undefined,
    })
    await route.fulfill({
      status: response.status,
      headers: Object.fromEntries(response.headers),
      body: await response.text(),
    })
  })
})
test.afterEach(() => server.resetHandlers())
test.afterAll(() => server.close())

test.describe('contract', () => {
  test('관리자 세션은 새로고침 후 유지되고 로그아웃하면 보호된 화면 접근이 차단된다', async ({
    page,
  }) => {
    const email = 'admin@example.test'
    const password = 'example-password'
    const token = 'admin-access-token'
    server.use(
      http.post('*/api/auth/web/login/local', () =>
        HttpResponse.json(
          {
            code: 'OK',
            data: { access_token: token },
          },
          { headers: { 'Set-Cookie': `test-session=${token}; Path=/; HttpOnly; SameSite=Strict` } },
        ),
      ),
      http.post('*/api/auth/web/logout', () =>
        HttpResponse.json(
          { code: 'OK' },
          { headers: { 'Set-Cookie': 'test-session=; Path=/; Max-Age=0' } },
        ),
      ),
      http.get('*/api/auth/me', ({ request }) => {
        if (request.headers.get('authorization') !== `Bearer ${token}`) {
          return HttpResponse.json({ code: 'UNAUTHORIZED' }, { status: 401 })
        }
        return HttpResponse.json({
          code: 'OK',
          data: {
            exp: Math.floor(Date.now() / 1000) + 900,
            user: { roles: ['admin'], permissions: ['rbac.read'] },
          },
        })
      }),
      http.get('*/api/rbac/roles', () => HttpResponse.json({ code: 'OK', data: [] })),
    )
    await page.goto('/login')
    await page.getByLabel('이메일').fill(email)
    await page.getByLabel('비밀번호').fill(password)
    await page.getByRole('button', { name: '로그인', exact: true }).click()
    await page.getByRole('link', { name: '관리', exact: true }).click()
    await page
      .getByRole('navigation', { name: '관리 메뉴' })
      .getByRole('link', { name: '역할 관리' })
      .click()
    await expect(page).toHaveURL('/admin/roles')
    const secondTab = await page.context().newPage()
    await secondTab.goto('/admin/roles')
    await expect(secondTab.getByRole('heading', { name: '역할 관리' })).toBeVisible()
    await secondTab.close()
    await page.reload()

    // 새로고침 후 브라우저에 저장된 세션으로 보호된 주소를 다시 연다.
    await expect(page.getByRole('heading', { name: '역할 관리' })).toBeVisible()
    await expect(page).toHaveURL('/admin/roles')

    await page.getByRole('button', { name: '로그아웃' }).click()

    // 보호된 하위 화면에서 로그아웃해도 홈으로 돌아간다.
    await expect(page).toHaveURL('/')
    await expect(page.getByRole('heading', { name: '홈' })).toBeVisible()
    await expect(page.getByRole('link', { name: '로그인' })).toBeVisible()
    await expect(page.getByRole('link', { name: '관리', exact: true })).toHaveCount(0)

    await page.goto('/admin/roles')

    // 로그아웃한 브라우저에서 보호된 주소를 열면 로그인 화면으로 이동한다.
    await expect(page).toHaveURL('/login')
    await expect(page.getByRole('heading', { name: '로그인' })).toBeVisible()
  })
})

test.describe('regression', () => {
  test('세션 저장소가 차단돼도 홈을 열고 현재 페이지에서 로그인·로그아웃할 수 있다', async ({
    page,
  }) => {
    await page.addInitScript(() => {
      Object.defineProperty(window, 'sessionStorage', {
        get() {
          throw new DOMException('Storage access denied', 'SecurityError')
        },
      })
    })
    server.use(
      http.post('*/api/auth/web/login/local', () =>
        HttpResponse.json(
          {
            data: { access_token: 'member-token' },
          },
          {
            headers: {
              'Set-Cookie': 'test-session=member-token; Path=/; HttpOnly; SameSite=Strict',
            },
          },
        ),
      ),
      http.post('*/api/auth/web/logout', () =>
        HttpResponse.json(
          { code: 'OK' },
          { headers: { 'Set-Cookie': 'test-session=; Path=/; Max-Age=0' } },
        ),
      ),
      http.get('*/api/auth/me', () =>
        HttpResponse.json({
          data: { exp: Math.floor(Date.now() / 1000) + 900, user: { roles: ['user'] } },
        }),
      ),
    )

    await page.goto('/')
    // 저장소 읽기가 실패해도 공개 홈과 로그인 메뉴를 표시한다.
    await expect(page.getByRole('heading', { name: '홈' })).toBeVisible()
    await page.getByRole('link', { name: '로그인' }).click()
    await page.getByLabel('이메일').fill('member@example.test')
    await page.getByLabel('비밀번호').fill('example-password')
    await page.getByRole('button', { name: '로그인', exact: true }).click()

    // 토큰을 저장할 수 없어도 현재 페이지에서 로그인 상태로 전환한다.
    await expect(page).toHaveURL('/')
    await page.getByRole('button', { name: '로그아웃' }).click()

    // 토큰을 삭제할 수 없어도 현재 세션을 종료한다.
    await expect(page.getByRole('link', { name: '로그인' })).toBeVisible()
    await expect(page.getByRole('button', { name: '로그아웃' })).toHaveCount(0)

    await page.getByRole('link', { name: '로그인' }).click()
    await page.getByLabel('이메일').fill('member@example.test')
    await page.getByLabel('비밀번호').fill('example-password')
    await page.getByRole('button', { name: '로그인', exact: true }).click()
    await expect(page.getByRole('button', { name: '로그아웃' })).toBeVisible()
    await page.reload()

    // 쿠키 세션은 저장소가 차단되어도 새로고침 후 복원된다.
    await expect(page.getByRole('button', { name: '로그아웃' })).toBeVisible()
  })
})

test('역할 상세는 높이가 바뀌어도 상단 정렬을 유지하고 모달을 닫는다', async ({ page }) => {
  const role = { id: 1, code: 'operator', name: '운영자', description: '' }
  server.use(
    http.post('*/api/auth/web/login/local', () =>
      HttpResponse.json(
        {
          code: 'OK',
          data: { access_token: 'token' },
        },
        { headers: { 'Set-Cookie': 'test-session=token; Path=/; HttpOnly; SameSite=Strict' } },
      ),
    ),
    http.get('*/api/auth/me', () =>
      HttpResponse.json({
        code: 'OK',
        data: {
          exp: Math.floor(Date.now() / 1000) + 900,
          user: {
            roles: ['admin'],
            permissions: ['rbac.read', 'auth.user.read', 'rbac.user_role.manage'],
          },
        },
      }),
    ),
    http.get('*/api/rbac/roles/1', () => HttpResponse.json({ code: 'OK', data: role })),
    http.get('*/api/rbac/roles/1/permissions', () => HttpResponse.json({ code: 'OK', data: [] })),
    http.get('*/api/rbac/permissions', () => HttpResponse.json({ code: 'OK', data: [] })),
    http.get('*/api/rbac/roles/1/users', () =>
      HttpResponse.json({ code: 'OK', data: { items: [], has_next: false } }),
    ),
    http.get('*/api/rbac/roles/1/users/search', () =>
      HttpResponse.json({
        code: 'OK',
        data: {
          items: [{ id: 3, nickname: '홍길동', emails: [], assigned: true }],
          has_next: false,
        },
      }),
    ),
  )
  await page.goto('/login')
  await page.getByLabel('이메일').fill('admin@example.test')
  await page.getByLabel('비밀번호').fill('example-password')
  await page.getByRole('button', { name: '로그인', exact: true }).click()
  await expect(page.getByRole('button', { name: '로그아웃' })).toBeVisible()
  await page.goto('/admin/roles/1')
  await page.getByRole('button', { name: '사용자 설정' }).click()
  const heading = page.getByRole('heading', { name: '운영자' })
  const before = await heading.boundingBox()
  await page.setViewportSize({ width: 1280, height: 1000 })
  const after = await heading.boundingBox()
  expect(Math.abs((after?.y ?? 0) - (before?.y ?? 0))).toBeLessThan(2)
  await page.getByRole('button', { name: '+ 사용자 추가' }).click()
  const dialog = page.getByRole('dialog', { name: '사용자 추가' })
  await expect(dialog).toBeVisible()
  await expect(dialog.getByRole('button', { name: '홍길동 할당됨' })).toBeDisabled()
  for (const width of [1280, 375]) {
    await page.setViewportSize({ width, height: 1000 })
    const input = await dialog.getByLabel('사용자 이름 검색').boundingBox()
    const button = await dialog.getByRole('button', { name: '검색', exact: true }).boundingBox()
    expect(input).not.toBeNull()
    expect(button).not.toBeNull()
    // 검색 입력과 버튼은 같은 높이로 한 줄에 표시된다.
    expect(Math.abs(input!.y - button!.y)).toBeLessThan(1)
    expect(Math.abs(input!.height - button!.height)).toBeLessThan(1)
    expect(button!.x).toBeGreaterThanOrEqual(input!.x + input!.width)
  }
  await page.keyboard.press('Escape')
  await expect(dialog).toBeHidden()
})
