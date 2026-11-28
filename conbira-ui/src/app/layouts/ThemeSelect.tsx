import { useState, type ChangeEvent } from 'react'
import { buttonVariants } from '@/shared/components/ui/Button'
import { cn } from '@/shared/lib/utils'

type Theme = 'system' | 'light' | 'dark'

const themeKey = 'conbira.theme'

export function ThemeSelect() {
  const [theme, setTheme] = useState<Theme>(() => {
    const preference = document.documentElement.dataset.theme
    return preference === 'light' || preference === 'dark' ? preference : 'system'
  })

  function changeTheme(event: ChangeEvent<HTMLSelectElement>) {
    const value = event.currentTarget.value
    if (value !== 'system' && value !== 'light' && value !== 'dark') return

    if (value === 'system') {
      delete document.documentElement.dataset.theme
    } else {
      document.documentElement.dataset.theme = value
    }
    setTheme(value)

    try {
      localStorage.setItem(themeKey, value)
    } catch {
      // The current page can still use the chosen theme.
    }
  }

  return (
    <div className="relative shrink-0">
      <select
        aria-label="화면 테마"
        value={theme}
        onChange={changeTheme}
        className={cn(
          buttonVariants({ variant: 'outline', size: 'sm' }),
          'block appearance-none py-0 pr-8 text-foreground',
        )}
      >
        <option value="system">시스템</option>
        <option value="light">라이트</option>
        <option value="dark">다크</option>
      </select>
      <span
        aria-hidden="true"
        className="pointer-events-none absolute end-3 top-1/2 size-1.5 -translate-y-2/3 rotate-45 border-e border-b border-current text-muted-foreground"
      />
    </div>
  )
}
