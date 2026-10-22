import { useEffect, useState } from 'react'
import { ResizeEdges } from '@/components/ui/window-frame'
import * as ipc from '@/ipc'
import type { Failure, VaultStatus } from '@/ipc'
import { Unlock } from '@/screens/Unlock'
import { Vault } from '@/screens/Vault'
import { Splash } from '@/shell/Splash'
import { Titlebar } from '@/shell/Titlebar'
import { text } from '@/text'

/**
 * Where the window is.
 *
 * - `starting`: asking which file it was started on. The splash.
 * - `locked`: the Unlock screen.
 * - `unsealing`: the password is being stretched. The splash again, over an
 *   Unlock screen that stands still beneath it, so a refusal comes back to
 *   the same file - but only once the wait is long enough to be seen
 *   (`shown`); see `SHOW_AFTER`.
 * - `open`: a vault is unlocked.
 */
type Stage =
  | { at: 'starting' }
  | { at: 'locked'; file: string | null; failure: Failure | null }
  | { at: 'unsealing'; file: string; shown: boolean }
  | { at: 'open'; status: VaultStatus }

/**
 * How long unsealing runs before the splash comes up, and how long it then
 * stays, in milliseconds.
 *
 * Stretching the password takes a tenth of a second on a fast machine and
 * well over one on a slow one. A splash that came up at once would blink on
 * the fast one - a flash of another screen and back, which reads as a fault -
 * so it waits until the wait is one a person would notice, and once up it
 * stays long enough to be read rather than glimpsed.
 */
export const SHOW_AFTER = 250
export const SHOW_FOR = 600

const pause = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms))

/** A rejection the bridge produced rather than the vault. */
function unexpected(cause: unknown): Failure {
  return { kind: 'other', message: cause instanceof Error ? cause.message : String(cause) }
}

export default function App() {
  const [stage, setStage] = useState<Stage>({ at: 'starting' })

  useEffect(() => {
    let current = true
    // A page that starts is a page that knows of no open vault, so there is
    // none: after a reload the vault the last page unlocked is locked, rather
    // than left open behind a screen that asks for its password.
    Promise.all([ipc.opening(), ipc.lock()])
      .then(([opening]) => opening)
      .then((opening) => current && setStage({ at: 'locked', file: opening.file, failure: null }))
      .catch(() => current && setStage({ at: 'locked', file: null, failure: null }))
    return () => {
      current = false
    }
  }, [])

  async function unlock(file: string, password: string) {
    setStage({ at: 'unsealing', file, shown: false })
    let shownAt: number | null = null
    const timer = setTimeout(() => {
      shownAt = Date.now()
      setStage((now) => (now.at === 'unsealing' ? { ...now, shown: true } : now))
    }, SHOW_AFTER)

    let next: Stage
    try {
      next = { at: 'open', status: await ipc.unlock(file, password) }
    } catch (cause) {
      next = { at: 'locked', file, failure: ipc.isFailure(cause) ? cause : unexpected(cause) }
    }

    clearTimeout(timer)
    if (shownAt !== null) {
      const left = SHOW_FOR - (Date.now() - shownAt)
      if (left > 0) await pause(left)
    }
    setStage(next)
  }

  async function lock() {
    // The screen goes first: whatever the answer, the vault is no longer on
    // it. The file stays offered, which is what a lock in the middle of the
    // day wants back.
    const file = stage.at === 'open' ? stage.status.path : null
    setStage({ at: 'locked', file, failure: null })
    await ipc.lock().catch(() => undefined)
  }

  if (stage.at === 'starting') return <Splash />

  return (
    <div className="flex h-full flex-col">
      <ResizeEdges />
      <Titlebar />
      {stage.at === 'open' ? (
        <Vault status={stage.status} onLock={() => void lock()} />
      ) : (
        <Unlock
          file={stage.file}
          failure={stage.at === 'locked' ? stage.failure : null}
          busy={stage.at === 'unsealing'}
          onUnlock={(file, password) => void unlock(file, password)}
        />
      )}
      {stage.at === 'unsealing' && stage.shown ? (
        <Splash status={text.splash.unsealing} tip={text.splash.unsealingTip} />
      ) : null}
    </div>
  )
}
