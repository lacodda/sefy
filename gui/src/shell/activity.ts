import * as ipc from '@/ipc'

/** How often at most the process hears of someone using the page, in
 * milliseconds. The idle time is measured in minutes; every keystroke would be
 * a call for nothing. */
export const REPORT_EVERY = 30_000

/** What counts as a person being there: pressing, typing, scrolling. Not the
 * pointer moving across the window on its way somewhere else. */
const ACTIVITY = ['pointerdown', 'keydown', 'wheel'] as const

/**
 * Tells the process when a person uses this page, so that the vault locks
 * after nobody has rather than after nothing was asked for: what the page asks
 * for on its own, a card's one-time code running on, must not keep a vault
 * unlocked in an empty room.
 *
 * Returns what stops it.
 */
export function reportActivity(): () => void {
  let last = Number.NEGATIVE_INFINITY

  const report = () => {
    const now = Date.now()
    if (now - last < REPORT_EVERY) return
    last = now
    ipc.touch().catch(() => undefined)
  }

  const options = { capture: true, passive: true }
  for (const type of ACTIVITY) window.addEventListener(type, report, options)
  return () => {
    for (const type of ACTIVITY) window.removeEventListener(type, report, options)
  }
}
