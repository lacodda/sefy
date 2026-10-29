import { useCallback, useEffect, useRef, useState, type KeyboardEvent } from 'react'
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'
import { cn } from 'dowel-ui'
import { ProductMark } from '@/components/ui/product-mark'
import { SearchField } from '@/components/ui/search-field'
import * as ipc from '@/ipc'
import type { Failure, ItemRow } from '@/ipc'
import { unexpected } from '@/lib/copies'
import { text } from '@/text'

/** The most a quick search shows: a line to pick from, not a list to read. */
export const SHOWN = 8

/** The event the process sends each time it brings this window up
 * (`quick::SHOWN` in src-tauri/src/quick.rs). */
export const SHOWN_EVENT = 'quick-shown'

/**
 * Quick search: a line over whatever is on the screen.
 *
 * Typing finds items the way the main window's search does, Enter puts the
 * chosen item's secret on the clipboard - what `sefy get` would take - and
 * the window goes away. Escape, or a click anywhere else, puts it away
 * without copying.
 *
 * It shows nothing until something is typed: brought up with someone else
 * looking at the screen, it should not open on a list of what the vault
 * holds. Each time it is brought up it starts again from an empty line.
 */
export function Quick() {
  const [query, setQuery] = useState('')
  const [rows, setRows] = useState<ItemRow[]>([])
  const [at, setAt] = useState(0)
  const [failure, setFailure] = useState<Failure | null>(null)
  const searchRef = useRef<HTMLInputElement>(null)

  const reset = useCallback(() => {
    setQuery('')
    setRows([])
    setAt(0)
    setFailure(null)
    searchRef.current?.focus()
  }, [])

  useEffect(() => {
    if (!('__TAURI_INTERNALS__' in window)) return
    const unlisten = getCurrentWebviewWindow().listen(SHOWN_EVENT, reset)
    return () => {
      void unlisten.then((stop) => stop())
    }
  }, [reset])

  const typed = query.trim()
  useEffect(() => {
    if (typed === '') return
    let current = true
    ipc
      .items({ text: typed, kind: null, tag: null })
      .then((found) => {
        if (!current) return
        setRows(found.slice(0, SHOWN))
        setAt(0)
      })
      .catch(() => current && setRows([]))
    return () => {
      current = false
    }
  }, [typed])

  // With the line empty there is nothing to pick from, whatever the last
  // answer was.
  const shown = typed === '' ? [] : rows

  const take = (row: ItemRow | undefined) => {
    if (row === undefined) return
    ipc
      .copy(row.id, null)
      .then(() => ipc.dismiss())
      .catch((cause: unknown) => setFailure(ipc.isFailure(cause) ? cause : unexpected(cause)))
  }

  const keys = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'Escape') {
      event.preventDefault()
      void ipc.dismiss()
    } else if (event.key === 'ArrowDown') {
      event.preventDefault()
      setAt((now) => Math.min(now + 1, Math.max(shown.length - 1, 0)))
    } else if (event.key === 'ArrowUp') {
      event.preventDefault()
      setAt((now) => Math.max(now - 1, 0))
    } else if (event.key === 'Enter') {
      event.preventDefault()
      take(shown[at])
    }
  }

  return (
    <main className="flex h-full flex-col gap-2 overflow-hidden rounded-lg border border-line bg-bg p-3">
      <div className="flex items-center gap-2.5">
        <ProductMark product="sefy" size={22} />
        <SearchField
          ref={searchRef}
          autoFocus
          value={query}
          onValueChange={(value) => {
            setQuery(value)
            setFailure(null)
          }}
          onKeyDown={keys}
          placeholder={text.quick.search}
          aria-label={text.quick.search}
          aria-controls="quick-results"
          aria-activedescendant={shown[at] === undefined ? undefined : `quick-${shown[at].id}`}
          className="flex-1"
        />
      </div>

      <ul id="quick-results" role="listbox" aria-label={text.quick.results} className="flex min-h-0 flex-1 flex-col">
        {shown.map((row, index) => (
          <li
            key={row.id}
            id={`quick-${row.id}`}
            role="option"
            aria-selected={index === at}
            onMouseEnter={() => setAt(index)}
            onClick={() => take(row)}
            className={cn(
              'flex cursor-pointer items-center gap-2.5 rounded-md px-2.5 py-1.5 text-sm',
              index === at ? 'bg-accent-soft text-text' : 'text-dim',
            )}
          >
            <span className="min-w-0 flex-1 truncate font-mono">{row.title}</span>
            <span className="font-mono text-2xs text-faint">{row.kind}</span>
          </li>
        ))}
        {typed !== '' && shown.length === 0 ? (
          <li className="px-2.5 py-1.5 text-sm text-dim">{text.quick.nothing}</li>
        ) : null}
      </ul>

      <p
        role={failure === null ? undefined : 'alert'}
        className={cn('px-1 text-2xs', failure === null ? 'text-faint' : 'text-bad')}
      >
        {failure === null ? text.quick.hint : failure.message}
      </p>
    </main>
  )
}
