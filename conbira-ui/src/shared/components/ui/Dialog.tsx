import { useEffect, useRef, type ReactNode } from 'react'

export function Dialog({
  children,
  labelledBy,
  onClose,
  busy = false,
  className,
}: {
  children: ReactNode
  labelledBy: string
  onClose: () => void
  busy?: boolean
  className?: string
}) {
  const dialog = useRef<HTMLDialogElement>(null)
  useEffect(() => {
    const element = dialog.current
    const previousFocus = document.activeElement
    if (!element) return
    if (typeof element.showModal === 'function') element.showModal()
    else element.setAttribute('open', '')
    return () => {
      if (element.open && typeof element.close === 'function') element.close()
      if (previousFocus instanceof HTMLElement && previousFocus.isConnected) previousFocus.focus()
    }
  }, [])
  return (
    <dialog
      ref={dialog}
      aria-labelledby={labelledBy}
      className={className}
      onCancel={(event) => {
        event.preventDefault()
        if (!busy) onClose()
      }}
    >
      {children}
    </dialog>
  )
}
