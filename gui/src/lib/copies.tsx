import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from 'react'
import * as ipc from '@/ipc'
import type { Failure } from '@/ipc'
import { cn } from 'dowel-ui'
import { Button } from '@/components/ui/button'
import { text } from '@/text'

/*
 * Copying, as the window shows it.
 *
 * The value never passes through the page: a Copy asks the process to put
 * the field on the clipboard, and the process takes it back after the hold
 * every front end shares (`sefy_core::clipboard::HOLD_SECONDS`). What the
 * page keeps is which button did it and until when, so that button can
 * count down in amber - the colour the line gives a secret that is out - and
 * every other one says Copy again: one secret is on the clipboard at a time,
 * and a second copy replaces the first.
 */

interface Held {
  /** Which button: the item and the field. */
  key: string
  /** When the clipboard is cleared, in milliseconds. */
  until: number
}

interface Copies {
  held: Held | null
  now: number
  /** Copies a field of an item; resolves to what went wrong, or null. */
  copy: (id: number, field: string | null) => Promise<Failure | null>
}

const CopiesContext = createContext<Copies | null>(null)

/** The name a field's button goes by. */
export const copyKey = (id: number, field: string | null) => `${id}:${field ?? ''}`

/** A rejection the bridge produced rather than the vault. */
export function unexpected(cause: unknown): Failure {
  return { kind: 'other', message: cause instanceof Error ? cause.message : String(cause) }
}

export function CopiesProvider({ children }: { children: ReactNode }) {
  const [held, setHeld] = useState<Held | null>(null)
  const [now, setNow] = useState(() => Date.now())

  useEffect(() => {
    if (held === null) return
    const timer = setInterval(() => {
      const at = Date.now()
      setNow(at)
      if (at >= held.until) setHeld(null)
    }, 250)
    return () => clearInterval(timer)
  }, [held])

  const copy = useCallback(async (id: number, field: string | null) => {
    try {
      const copied = await ipc.copy(id, field)
      const at = Date.now()
      setNow(at)
      setHeld({ key: copyKey(id, field), until: at + copied.seconds * 1000 })
      return null
    } catch (cause) {
      return ipc.isFailure(cause) ? cause : unexpected(cause)
    }
  }, [])

  const value = useMemo(() => ({ held, now, copy }), [held, now, copy])
  return <CopiesContext.Provider value={value}>{children}</CopiesContext.Provider>
}

export function useCopies(): Copies {
  const copies = useContext(CopiesContext)
  if (copies === null) throw new Error('useCopies needs a CopiesProvider above it')
  return copies
}

interface CopyButtonProps {
  id: number
  field: string | null
  /** What the button is called for a screen reader: "Copy password". */
  label: string
  /** Told what went wrong, when something did. */
  onFailure: (failure: Failure) => void
  /** The button's own words, when not the plain "Copy". */
  children?: ReactNode
  className?: string
}

/** Copy, or the seconds until the clipboard is cleared. */
export function CopyButton({ id, field, label, onFailure, children, className }: CopyButtonProps) {
  const { held, now, copy } = useCopies()
  const counting = held !== null && held.key === copyKey(id, field)
  const left = counting ? Math.max(1, Math.ceil((held.until - now) / 1000)) : 0

  return (
    <Button
      size="sm"
      aria-label={label}
      onClick={() => {
        void copy(id, field).then((failure) => failure && onFailure(failure))
      }}
      className={cn(
        'w-18 shrink-0 font-mono tabular-nums',
        // Amber while the secret is out: the line's colour for a secret
        // somewhere other than the vault.
        counting && 'border-warn bg-warn-soft text-warn not-data-disabled:hover:border-warn not-data-disabled:hover:text-warn',
        className,
      )}
    >
      {counting ? text.card.copied(left) : (children ?? text.card.copy)}
    </Button>
  )
}
