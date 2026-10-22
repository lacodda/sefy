import { useEffect, useRef, useState, type FormEvent } from 'react'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import { cn } from 'dowel-ui'
import { Alert } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { fieldClasses } from '@/components/ui/input'
import { PasswordField } from '@/components/ui/password-field'
import { chooseFile, type Failure } from '@/ipc'
import { text } from '@/text'

interface Props {
  /** The file to offer, when the window was started on one or the last
   * attempt named one. */
  file: string | null
  /** Why the last attempt did not open it. */
  failure: Failure | null
  /** Whether an attempt is on its way: the form stands still under the
   * splash, so that a refusal comes back to the same screen. */
  busy: boolean
  onUnlock: (file: string, password: string) => void
}

/** The folder and the name of a path, split where the last separator is. */
function split(path: string): { folder: string; name: string } {
  const at = Math.max(path.lastIndexOf('\\'), path.lastIndexOf('/'))
  return { folder: path.slice(0, at + 1), name: path.slice(at + 1) }
}

/** What to say about a failure; the core's own words when there is nothing
 * more useful to add. */
function explain(failure: Failure): string {
  switch (failure.kind) {
    case 'refused':
      return text.unlock.refused
    case 'missing':
      return text.unlock.missing
    case 'newer-format':
      return text.unlock.newerFormat
    default:
      return failure.message
  }
}

/**
 * The sign of the product as the state of the screen: the mark's "password
 * dots", two lit and the third still being typed.
 */
function Sign() {
  return (
    <div aria-hidden className="flex justify-center gap-3">
      <i className="size-3.5 rounded-full bg-accent" />
      <i className="size-3.5 rounded-full bg-accent" />
      <i className="size-3.5 animate-pulse rounded-full border-[3px] border-accent" />
    </div>
  )
}

/**
 * Open a vault: a file and a password.
 *
 * The file is chosen as any file is - the system's dialog with no filter, a
 * file dropped on the window, or the one the window was started on - because a
 * vault is any file. There is no list of vaults opened before: sefy keeps no
 * record of where its vaults are, and a list on this screen would be that
 * record (ADR 0010).
 */
export function Unlock({ file: offered, failure, busy, onUnlock }: Props) {
  const [file, setFile] = useState<string | null>(offered)
  const [password, setPassword] = useState('')
  const [dragging, setDragging] = useState(false)
  const passwordRef = useRef<HTMLInputElement>(null)

  // After a refusal the next attempt types over the password, and a fresh
  // screen with a file already named starts in the password.
  useEffect(() => {
    if (busy) return
    if (failure?.kind === 'refused') {
      passwordRef.current?.focus()
      passwordRef.current?.select()
    } else if (file !== null) {
      passwordRef.current?.focus()
    }
  }, [busy, failure, file])

  // A file dropped anywhere on the window is the file to open.
  useEffect(() => {
    let stop: (() => void) | undefined
    let stopped = false
    if (!('__TAURI_INTERNALS__' in window)) return
    getCurrentWebview()
      .onDragDropEvent((event) => {
        const drag = event.payload
        if (drag.type === 'enter' || drag.type === 'over') setDragging(true)
        else if (drag.type === 'leave') setDragging(false)
        else if (drag.type === 'drop') {
          setDragging(false)
          const dropped = drag.paths[0]
          if (dropped !== undefined) setFile(dropped)
        }
      })
      .then((unlisten) => {
        if (stopped) unlisten()
        else stop = unlisten
      })
      .catch(() => undefined)
    return () => {
      stopped = true
      stop?.()
    }
  }, [])

  async function browse() {
    const chosen = await chooseFile(file).catch(() => null)
    if (chosen !== null) setFile(chosen)
  }

  function submit(event: FormEvent) {
    event.preventDefault()
    if (busy || file === null || password === '') return
    onUnlock(file, password)
    // The page keeps no copy once it is on its way: a refusal is typed again.
    setPassword('')
  }

  const shown = file === null ? null : split(file)

  return (
    <main
      inert={busy}
      className="flex min-h-0 flex-1 flex-col items-center justify-center overflow-y-auto px-6 py-10"
    >
      <form onSubmit={submit} className="flex w-105 max-w-full flex-col gap-3">
        <Sign />
        <h1 className="mt-4 mb-4 text-center text-lg font-semibold text-text">{text.unlock.title}</h1>

        <div className="flex gap-2">
          <div
            role="group"
            aria-label={text.unlock.file}
            data-dragging={dragging || undefined}
            className={cn(
              fieldClasses,
              'flex h-control min-w-0 flex-1 items-center gap-0 overflow-hidden font-mono whitespace-nowrap',
              dragging && 'border-accent',
            )}
          >
            {dragging ? (
              <span className="text-accent">{text.unlock.dropHere}</span>
            ) : shown === null ? (
              <span className="text-faint">{text.unlock.noFile}</span>
            ) : (
              <>
                <span className="min-w-0 truncate text-faint" title={file ?? undefined}>
                  {shown.folder}
                </span>
                <span className="shrink-0 text-text">{shown.name}</span>
              </>
            )}
          </div>
          <Button type="button" onClick={() => void browse()}>
            {text.unlock.browse}
          </Button>
        </div>

        <div className="flex gap-2">
          <PasswordField
            ref={passwordRef}
            aria-label={text.unlock.password}
            placeholder={text.unlock.password}
            // Not `current-password`: that is the word a browser's password
            // manager waits for, and a master password has no business in it.
            autoComplete="off"
            showLabel={text.unlock.show}
            hideLabel={text.unlock.hide}
            value={password}
            onValueChange={setPassword}
            className="flex-1 font-mono"
          />
          <Button type="submit" variant="primary" disabled={file === null || password === ''}>
            {text.unlock.submit}
          </Button>
        </div>

        {/* Announced, not only drawn: the focus has gone back to the
            password, and someone listening there would otherwise hear
            nothing about why. */}
        {failure === null || busy ? null : (
          <Alert tone="bad" role="alert">
            {explain(failure)}
          </Alert>
        )}

        <p className="mt-6 text-center text-xs text-faint">{text.unlock.foot}</p>
      </form>
    </main>
  )
}
