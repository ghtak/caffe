import { QueryClient } from '@tanstack/react-query'
import { createMemoryHistory } from '@tanstack/react-router'
import { render } from '@testing-library/react'
import { App } from '../src/app/App'
import { createAppRouter } from '../src/app/routes/router'

export function renderWithProviders(path: string, accessToken?: string) {
  if (accessToken) sessionStorage.setItem('conbira.access-token', accessToken)
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: Infinity } },
  })
  const router = createAppRouter(queryClient, createMemoryHistory({ initialEntries: [path] }))
  return { ...render(<App appRouter={router} appQueryClient={queryClient} />), router }
}
