import { useLayoutEffect, useRef, type ReactNode } from 'react'

export function Dialog({ children, close, label, className }: {
  children: ReactNode; close: () => void; label: string; className?: string
}) {
  const ref = useRef<HTMLDialogElement>(null)
  useLayoutEffect(() => {
    const element = ref.current
    element?.showModal()
    return () => element?.close()
  }, [])

  return <dialog ref={ref} className={className} aria-label={label} onCancel={event => {
    event.preventDefault()
    close()
  }}>{children}</dialog>
}
