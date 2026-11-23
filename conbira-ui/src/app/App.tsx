import { QueryClient, QueryClientProvider, useQueryClient } from '@tanstack/react-query'
import { RouterProvider } from '@tanstack/react-router'
import { SessionProvider, useSession } from './providers/SessionProvider'
import { createAppRouter } from './routes/router'

const queryClient = new QueryClient()
const router = createAppRouter(queryClient)

function AppRoutes({ appRouter }: { appRouter: typeof router }) {
  const { session } = useSession()
  const appQueryClient = useQueryClient()
  return <RouterProvider router={appRouter} context={{ queryClient: appQueryClient, session }} />
}

export function App({
  appRouter = router,
  appQueryClient = queryClient,
}: { appRouter?: typeof router; appQueryClient?: QueryClient } = {}) {
  return (
    <QueryClientProvider client={appQueryClient}>
      <SessionProvider>
        <AppRoutes appRouter={appRouter} />
      </SessionProvider>
    </QueryClientProvider>
  )
}
