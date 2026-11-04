import { mockIPC, mockWindows } from '@tauri-apps/api/mocks'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { REPORT_EVERY, reportActivity } from '@/shell/activity'

describe('someone using the page', () => {
  let touches = 0
  let stop: () => void = () => undefined

  beforeEach(() => {
    touches = 0
    vi.useFakeTimers()
    mockWindows('main')
    mockIPC((cmd) => {
      if (cmd === 'touch') touches += 1
      return null
    })
    stop = reportActivity()
  })

  afterEach(() => {
    stop()
    vi.useRealTimers()
  })

  it('is what the process hears about', () => {
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'a' }))
    expect(touches).toBe(1)
  })

  it.each(['pointerdown', 'keydown', 'wheel'])('shows in a %s', (type) => {
    window.dispatchEvent(new Event(type))
    expect(touches).toBe(1)
  })

  it('does not show in the pointer passing over the window', () => {
    window.dispatchEvent(new Event('pointermove'))
    window.dispatchEvent(new Event('mousemove'))
    expect(touches).toBe(0)
  })

  it('is heard about once in a while, not on every keystroke', () => {
    window.dispatchEvent(new Event('keydown'))
    window.dispatchEvent(new Event('keydown'))
    vi.advanceTimersByTime(REPORT_EVERY - 1)
    window.dispatchEvent(new Event('keydown'))
    expect(touches).toBe(1)

    vi.advanceTimersByTime(1)
    window.dispatchEvent(new Event('keydown'))
    expect(touches).toBe(2)
  })

  it('is often enough for the idle time, which is measured in minutes', () => {
    expect(REPORT_EVERY).toBeLessThanOrEqual(60_000)
  })

  it('is no longer heard about once it is stopped', () => {
    stop()
    window.dispatchEvent(new Event('keydown'))
    expect(touches).toBe(0)
  })
})
