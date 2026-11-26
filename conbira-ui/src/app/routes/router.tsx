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
  return (
    <UserListPage token={session.token} canEdit={can(session.permissions, ['auth.user.manage'])} />
  )
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
  return <UserDetailPage id={id} token={session.token} />
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
const routeTree = rootRoute.addChildren([
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
