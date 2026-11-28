import * as React from 'react'
import { cn } from '@/shared/lib/utils'

function EmptyState({
  children,
  action,
  className,
}: {
  children: React.ReactNode
  action?: React.ReactNode
  className?: string
}) {
  return (
    <div
      data-slot="empty-state"
      className={cn(
        'grid justify-items-center gap-3 rounded-lg border border-dashed bg-muted/30 p-8 text-center text-sm text-muted-foreground',
        className,
      )}
    >
      <div>{children}</div>
      {action}
    </div>
  )
}

export { EmptyState }
