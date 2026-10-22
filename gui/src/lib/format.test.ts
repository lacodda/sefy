import { describe, expect, it } from 'vitest'
import { ago, count, size } from './format'

describe('size', () => {
  it('reads the way the command line prints it', () => {
    expect(size(512)).toBe('512 B')
    expect(size(86_118)).toBe('84.1 KB')
    expect(size(2_202_009)).toBe('2.1 MB')
  })
})

describe('count', () => {
  it('says one and many', () => {
    expect(count(1, 'item', 'items')).toBe('1 item')
    expect(count(0, 'item', 'items')).toBe('0 items')
  })
})

describe('ago', () => {
  const now = 1_800_000_000

  it('counts back in the largest whole unit', () => {
    expect(ago(now - 30, now)).toBe('now')
    expect(ago(now - 120, now)).toBe('2 minutes ago')
    expect(ago(now - 7200, now)).toBe('2 hours ago')
    expect(ago(now - 86_400, now)).toBe('yesterday')
  })

  it('never claims a moment that has not happened', () => {
    // A clock ahead of this machine's: the stamp is a few minutes "later".
    expect(ago(now + 300, now)).toBe('now')
  })
})
