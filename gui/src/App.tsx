import { useEffect, useState } from 'react'
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'
import { ResizeEdges } from '@/components/ui/window-frame'
import * as ipc from '@/ipc'
import type { Failure, VaultStatus, WipeOutcome } from '@/ipc'
import { Unlock } from '@/screens/Unlock'
import { Vault } from '@/screens/Vault'
import { reportActivity } from '@/shell/activity'
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
 * - `open`: a vault is unlocked. `revision` goes up when what it holds may have
 *   moved on, and `unreadable` says the file changed into something the window
 *   cannot read.
 */
type Stage =
  | { at: 'starting' }
  | { at: 'locked'; file: string | null; failure: Failure | null; notice?: string }
  | { at: 'unsealing'; file: string; shown: boolean }
  | { at: 'open'; status: VaultStatus; revision: number; unreadable: boolean }

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

/** The event the process sends when the vault locked without the page asking:
 * the time ran out, or `sefy lock` came from outside (`session::LOCKED`). */
export const LOCKED_EVENT = 'vault-locked'

/** The event the process sends when a second launch handed over a file
 * (`session::HANDED_OVER`). */
export const HANDED_OVER_EVENT = 'file-handed-over'

/** The event the process sends when what the page shows may have moved on: the
 * file was written, from here or from elsewhere (`session::CHANGED`). */
export const CHANGED_EVENT = 'vault-changed'

/** The event the process sends when the file changed and it cannot read the new
 * version (`session::UNREADABLE`). */
export const UNREADABLE_EVENT = 'vault-unreadable'

const pause = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms))

/** A rejection the bridge produced rather than the vault. */
function unexpected(cause: unknown): Failure {
  return { kind: 'other', message: cause instanceof Error ? cause.message : String(cause) }
}

export default function App() {
  const [stage, setStage] = useState<Stage>({ at: 'starting' })
  // How many files were handed over: the Unlock screen keeps a file it was
  // given in its own state, and starts over on the one handed over.
  const [handovers, setHandovers] = useState(0)

  const unlocked = stage.at === 'open'
  useEffect(() => (unlocked ? reportActivity() : undefined), [unlocked])

  useEffect(() => {
    let current = true

    // What a second launch left, which the screen takes if it is the one
    // asking for a file. With a vault open the process has already seen to
    // whether that vault gives way.
    const takeHandover = () =>
      ipc
        .takeHandover()
        .then((file) => {
          if (!current || file === null) return
          setStage((now) => (now.at === 'locked' ? { at: 'locked', file, failure: null } : now))
          setHandovers((count) => count + 1)
        })
        .catch(() => undefined)

    const page = getCurrentWebviewWindow()
    // The vault going without this page asking leaves the screen to catch up.
    // The file stays offered, as it does after the Lock button.
    const stopLocked = page.listen(LOCKED_EVENT, () => {
      setStage((now) => (now.at === 'open' ? { at: 'locked', file: now.status.path, failure: null } : now))
    })
    const stopHanded = page.listen(HANDED_OVER_EVENT, () => void takeHandover())
    const stopChanged = page.listen(CHANGED_EVENT, () => {
      ipc
        .status()
        .then((status) => {
          if (current) {
            setStage((now) =>
              now.at === 'open' ? { at: 'open', status, revision: now.revision + 1, unreadable: false } : now,
            )
          }
        })
        .catch(() => undefined)
    })
    const stopUnreadable = page.listen(UNREADABLE_EVENT, () => {
      setStage((now) => (now.at === 'open' ? { ...now, unreadable: true } : now))
    })

    // A page that starts is a page that knows of no open vault, so there is
    // none: after a reload the vault the last page unlocked is locked, rather
    // than left open behind a screen that asks for its password.
    Promise.all([ipc.opening(), ipc.lock()])
      .then(([opening]) => opening)
      .then((opening) => current && setStage({ at: 'locked', file: opening.file, failure: null }))
      .catch(() => current && setStage({ at: 'locked', file: null, failure: null }))
      // Once it is listening and on its feet: a launch that came sooner sent
      // its event to nobody, and left the file all the same.
      .then(() => Promise.all([stopLocked, stopHanded]))
      .then(() => takeHandover())
      .catch(() => undefined)

    return () => {
      current = false
      void stopLocked.then((stop) => stop())
      void stopHanded.then((stop) => stop())
      void stopChanged.then((stop) => stop())
      void stopUnreadable.then((stop) => stop())
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
      next = { at: 'open', status: await ipc.unlock(file, password), revision: 0, unreadable: false }
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

  // The screen is told what is left, and starts over asking for a file.
  function wiped(outcome: WipeOutcome) {
    setHandovers((count) => count + 1)
    setStage({ at: 'locked', file: null, failure: null, notice: text.wiped(outcome) })
  }

  if (stage.at === 'starting') return <Splash />

  return (
    <div className="flex h-full flex-col">
      <ResizeEdges />
      <Titlebar />
      {stage.at === 'open' ? (
        <Vault
          status={stage.status}
          revision={stage.revision}
          unreadable={stage.unreadable}
          onLock={() => void lock()}
          onWiped={wiped}
        />
      ) : (
        <Unlock
          key={handovers}
          file={stage.file}
          failure={stage.at === 'locked' ? stage.failure : null}
          notice={stage.at === 'locked' ? stage.notice : undefined}
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
