import type { QueryClient } from '@tanstack/react-query'
import {
  createRootRouteWithContext,
  createRoute,
  createRouter,
  Navigate,
  type RouterHistory,
} from '@tanstack/react-router'
import { z } from 'zod'
import { AdminHomePage } from '@/features/admin'
import { GoogleCallbackPage, LoginPage } from '@/features/auth'
import { HomePage } from '@/features/home'
import { PostCreatePage, PostDetailPage, PostListPage } from '@/features/board'
import { CommunityLayout } from '../layouts/CommunityLayout'
import { FileAssetPage } from '@/features/file_asset'
import { AdminCatalogPage, RoleDetailPage, catalogOptions, type Catalog } from '@/features/rbac'
import { UserDetailPage, UserListPage } from '@/features/users'
import { can, visibleAdminSections } from '../access'
import { AdminLayout } from '../layouts/AdminLayout'
import { RootLayout } from '../layouts/RootLayout'
import { useSession, type Session } from '../providers/SessionProvider'

type RouterContext = { queryClient: QueryClient; session: Session }

function LoginRoute() {
  const { session, signIn } = useSession()
  if (session.status === 'authenticated') return <Navigate to="/" replace />
  if (session.status !== 'guest') return <main className="min-h-90" />
  return <LoginPage onLogin={signIn} />
}

function AdminRoute() {
  const { session } = useSession()
  if (session.status === 'guest') return <Navigate to="/login" replace />
  if (session.status !== 'authenticated') return <main className="min-h-90" />
  if (visibleAdminSections(session.permissions).length === 0) return <Navigate to="/" replace />
  return <AdminLayout />
}

function AdminCatalogRoute({ catalog }: { catalog: Catalog }) {
  const { session } = useSession()
  if (session.status !== 'authenticated') return null
  if (!can(session.permissions, ['rbac.read']))
    return (
      <p role="alert" className="p-8">
        목록을 조회할 권한이 없습니다.
      </p>
    )
  return <AdminCatalogPage catalog={catalog} token={session.token} />
}

function AdminUsersRoute() {
  const { session } = useSession()
  if (session.status !== 'authenticated') return null
  if (!can(session.permissions, ['auth.user.read']))
    return (
      <p role="alert" className="p-8">
        사용자 목록을 조회할 권한이 없습니다.
      </p>
    )
  return <UserListPage token={session.token} />
}

function loadCatalog({ queryClient, session }: RouterContext, catalog: Catalog) {
  if (session.status !== 'authenticated' || !can(session.permissions, ['rbac.read'])) return
  // The page owns pending and error UI while the query is warmed.
  void queryClient.ensureQueryData(catalogOptions(catalog, session.token)).catch(() => {})
}

const rootRoute = createRootRouteWithContext<RouterContext>()({ component: RootLayout })
const indexRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/',
  component: HomePage,
})
const loginRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/login',
  component: LoginRoute,
})
const googleCallbackRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/login/google/callback',
  validateSearch: z.object({
    code: z.string().optional().catch(undefined),
    state: z.string().optional().catch(undefined),
    error: z.string().optional().catch(undefined),
  }),
  component: GoogleCallbackRoute,
})

function GoogleCallbackRoute() {
  const search = googleCallbackRoute.useSearch()
  const { signInGoogle, session } = useSession()
  const navigate = googleCallbackRoute.useNavigate()
  if (session.status === 'loading') return <main className="min-h-90" />
  return (
    <GoogleCallbackPage
      search={search}
      signIn={signInGoogle}
      clearUrl={() => void navigate({ search: {}, replace: true })}
    />
  )
}
const adminRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/admin',
  component: AdminRoute,
})
const adminIndexRoute = createRoute({
  getParentRoute: () => adminRoute,
  path: '/',
  component: () => <AdminHomeRoute />,
})

function AdminHomeRoute() {
  const { session } = useSession()
  return session.status === 'authenticated' ? (
    <AdminHomePage sections={visibleAdminSections(session.permissions)} />
  ) : null
}
const rolesRoute = createRoute({
  getParentRoute: () => adminRoute,
  path: 'roles',
  loader: ({ context }) => loadCatalog(context, 'roles'),
  component: () => <AdminCatalogRoute catalog="roles" />,
})
function RoleDetailRoute() {
  const { roleId } = roleDetailRoute.useParams()
  const { session } = useSession()
  const id = Number(roleId)
  if (session.status !== 'authenticated') return null
  if (!Number.isSafeInteger(id) || id < 1)
    return (
      <p role="alert" className="p-8">
        역할을 찾을 수 없습니다.
      </p>
    )
  if (!can(session.permissions, ['rbac.read']))
    return (
      <p role="alert" className="p-8">
        역할을 조회할 권한이 없습니다.
      </p>
    )
  return (
    <RoleDetailPage
      id={id}
      token={session.token}
      canEditPermissions={can(session.permissions, ['rbac.role.manage'])}
      canReadUsers={can(session.permissions, ['rbac.read', 'auth.user.read'])}
      canEditUsers={can(session.permissions, ['rbac.user_role.manage'])}
    />
  )
}
const roleDetailRoute = createRoute({
  getParentRoute: () => adminRoute,
  path: 'roles/$roleId',
  component: RoleDetailRoute,
})
const usersRoute = createRoute({
  getParentRoute: () => adminRoute,
  path: 'users',
  component: AdminUsersRoute,
})
function UserDetailRoute() {
  const { userId } = userDetailRoute.useParams()
  const { session } = useSession()
  if (session.status !== 'authenticated') return null
  if (!can(session.permissions, ['auth.user.read']))
    return (
      <p role="alert" className="p-8">
        사용자를 조회할 권한이 없습니다.
      </p>
    )
  const id = Number(userId)
  if (!/^-?\d+$/.test(userId) || !Number.isSafeInteger(id))
    return (
      <p role="alert" className="p-8">
        사용자를 찾을 수 없습니다.
      </p>
    )
  return (
    <UserDetailPage
      key={id}
      id={id}
      token={session.token}
      canEdit={can(session.permissions, ['auth.user.manage'])}
    />
  )
}
const userDetailRoute = createRoute({
  getParentRoute: () => adminRoute,
  path: 'users/$userId',
  component: UserDetailRoute,
})
const permissionsRoute = createRoute({
  getParentRoute: () => adminRoute,
  path: 'permissions',
  loader: ({ context }) => loadCatalog(context, 'permissions'),
  component: () => <AdminCatalogRoute catalog="permissions" />,
})
function AdminFileAssetsRoute() {
  const { session } = useSession()
  if (session.status !== 'authenticated') return null
  if (!can(session.permissions, ['file_asset.list.read']))
    return (
      <p role="alert" className="p-8">
        파일 에셋 목록을 조회할 권한이 없습니다.
      </p>
    )
  return (
    <FileAssetPage
      token={session.token}
      canManage={can(session.permissions, ['file_asset.manage'])}
    />
  )
}
const fileAssetsRoute = createRoute({
  getParentRoute: () => adminRoute,
  path: 'file-assets',
  component: AdminFileAssetsRoute,
})
const communityRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/community',
  component: CommunityLayout,
})
const communityIndexRoute = createRoute({
  getParentRoute: () => communityRoute,
  path: '/',
  component: () => <Navigate to="/community/notice" replace />,
})
const noticeRoute = createRoute({
  getParentRoute: () => communityRoute,
  path: 'notice',
  component: () => <BoardListRoute boardSlug="notice" title="공지" />,
})
const freeRoute = createRoute({
  getParentRoute: () => communityRoute,
  path: 'free',
  component: () => <BoardListRoute boardSlug="free" title="자유게시판" />,
})
const dataRoute = createRoute({
  getParentRoute: () => communityRoute,
  path: 'data',
  component: () => <BoardListRoute boardSlug="data" title="자료실" />,
})
function BoardListRoute({
  boardSlug,
  title,
}: {
  boardSlug: 'notice' | 'free' | 'data'
  title: string
}) {
  const { session } = useSession()
  const canWrite =
    session.status === 'authenticated' && can(session.permissions, [`board.${boardSlug}.write`])
  return <PostListPage key={boardSlug} boardSlug={boardSlug} title={title} canWrite={canWrite} />
}
function BoardCreateRoute({
  boardSlug,
  title,
}: {
  boardSlug: 'notice' | 'free' | 'data'
  title: string
}) {
  const { session } = useSession()
  if (session.status === 'guest') return <Navigate to="/login" replace />
  if (session.status !== 'authenticated') return <p role="status">로그인 상태 확인 중…</p>
  if (!can(session.permissions, [`board.${boardSlug}.write`]))
    return (
      <p role="alert" className="p-8">
        글을 작성할 권한이 없습니다.
      </p>
    )
  return (
    <PostCreatePage key={boardSlug} boardSlug={boardSlug} title={title} token={session.token} />
  )
}
const noticeCreateRoute = createRoute({
  getParentRoute: () => communityRoute,
  path: 'notice/new',
  component: () => <BoardCreateRoute boardSlug="notice" title="공지" />,
})
const freeCreateRoute = createRoute({
  getParentRoute: () => communityRoute,
  path: 'free/new',
  component: () => <BoardCreateRoute boardSlug="free" title="자유게시판" />,
})
const dataCreateRoute = createRoute({
  getParentRoute: () => communityRoute,
  path: 'data/new',
  component: () => <BoardCreateRoute boardSlug="data" title="자료실" />,
})
const postDetailRoute = createRoute({
  getParentRoute: () => communityRoute,
  path: 'posts/$postId',
  component: PostDetailRoute,
})
function PostDetailRoute() {
  const { postId } = postDetailRoute.useParams()
  const { session } = useSession()
  const id = Number(postId)
  if (!/^[1-9]\d*$/.test(postId) || !Number.isSafeInteger(id))
    return (
      <p role="alert" className="p-8">
        게시글을 찾을 수 없습니다.
      </p>
    )
  return (
    <PostDetailPage
      id={id}
      token={session.status === 'authenticated' ? session.token : undefined}
      userId={session.status === 'authenticated' ? session.userId : undefined}
    />
  )
}
const routeTree = rootRoute.addChildren([
  communityRoute.addChildren([
    communityIndexRoute,
    noticeRoute,
    freeRoute,
    dataRoute,
    noticeCreateRoute,
    freeCreateRoute,
    dataCreateRoute,
    postDetailRoute,
  ]),
  indexRoute,
  loginRoute,
  googleCallbackRoute,
  adminRoute.addChildren([
    adminIndexRoute,
    usersRoute,
    userDetailRoute,
    rolesRoute,
    roleDetailRoute,
    permissionsRoute,
    fileAssetsRoute,
  ]),
])

export function createAppRouter(queryClient: QueryClient, history?: RouterHistory) {
  return createRouter({
    routeTree,
    history,
    context: { queryClient, session: { status: 'guest' } },
  })
}

declare module '@tanstack/react-router' {
  interface Register {
    router: ReturnType<typeof createAppRouter>
  }
}
