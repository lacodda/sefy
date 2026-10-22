/*
 * Numbers and moments the way a person reads them. Presentation only: what is
 * counted and when it happened come from the core, and only the words are
 * chosen here.
 */

/** A number of bytes: `512 B`, `84.1 KB`, `2.1 MB`. The units the command
 * line's `status` prints. */
export function size(bytes: number): string {
  const kb = 1024
  const mb = 1024 * kb
  if (bytes < kb) return `${bytes} B`
  if (bytes < mb) return `${(bytes / kb).toFixed(1)} KB`
  return `${(bytes / mb).toFixed(1)} MB`
}

/** `1 item`, `24 items`. */
export function count(n: number, one: string, many: string): string {
  return `${n} ${n === 1 ? one : many}`
}

const relative = new Intl.RelativeTimeFormat('en', { numeric: 'auto' })

const STEPS: [Intl.RelativeTimeFormatUnit, number][] = [
  ['year', 365 * 24 * 3600],
  ['month', 30 * 24 * 3600],
  ['week', 7 * 24 * 3600],
  ['day', 24 * 3600],
  ['hour', 3600],
  ['minute', 60],
]

/** How long ago a moment was: `2 hours ago`, `yesterday`, `now`.
 *
 * `at` and `now` are Unix seconds. A moment in the future - a clock that
 * disagreed with another machine's - reads as `now` rather than as `in 3
 * minutes`, which would be a claim about something that has not happened. */
export function ago(at: number, now: number): string {
  const elapsed = Math.max(0, now - at)
  for (const [unit, seconds] of STEPS) {
    if (elapsed >= seconds) return relative.format(-Math.floor(elapsed / seconds), unit)
  }
  return relative.format(0, 'second')
}

/** Unix seconds now. */
export const nowSeconds = () => Math.floor(Date.now() / 1000)
