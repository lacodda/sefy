import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { emit } from '@tauri-apps/api/event'
import { mockIPC, mockWindows } from '@tauri-apps/api/mocks'
import { describe, expect, it } from 'vitest'
import type { Failure, Filter, ItemRow } from '@/ipc'
import { Quick, SHOWN, SHOWN_EVENT } from '@/screens/Quick'

const ROWS: ItemRow[] = [
  { id: 2, title: 'mailbox', kind: 'login', known: true, tags: [], updatedAt: 1_690_000_000 },
  { id: 7, title: 'mailroom', kind: 'login', known: true, tags: [], updatedAt: 1_680_000_000 },
  { id: 9, title: 'scan.pdf', kind: 'file', known: true, tags: [], updatedAt: 1_670_000_000 },
]

const A_FILE: Failure = { kind: 'other', message: '"scan.pdf" is a file; files leave the vault with sefy extract' }

function bridge() {
  const calls: [string, Record<string, unknown> | undefined][] = []
  mockWindows('quick')
  mockIPC((cmd, args) => {
    const given = args as Record<string, unknown> | undefined
    calls.push([cmd, given])
    switch (cmd) {
      case 'items': {
        const filter = given!.filter as Filter
        return [...ROWS, ...ROWS, ...ROWS, ...ROWS].filter((row) => row.title.includes(filter.text ?? ''))
      }
      case 'copy':
        if (given!.id === 9) throw A_FILE
        return { what: 'password', seconds: 45 }
      default:
        return null
    }
  }, { shouldMockEvents: true })
  const named = (cmd: string) => calls.filter(([name]) => name === cmd)
  return { calls, named }
}

describe('quick search', () => {
  it('shows nothing until something is typed', async () => {
    const bridged = bridge()
    render(<Quick />)

    await waitFor(() => expect(screen.getByRole('searchbox')).toHaveFocus())
    expect(screen.queryAllByRole('option')).toHaveLength(0)
    expect(bridged.named('items')).toHaveLength(0)
  })

  it('finds as it is typed, and shows a short list', async () => {
    const bridged = bridge()
    render(<Quick />)

    await userEvent.type(screen.getByRole('searchbox'), 'mail')
    await waitFor(() => expect(screen.getAllByRole('option')).toHaveLength(SHOWN))
    expect(bridged.named('items').at(-1)).toEqual(['items', { filter: { text: 'mail', kind: null, tag: null } }])
    expect(screen.getAllByRole('option')[0]).toHaveAttribute('aria-selected', 'true')
  })

  it('copies the first match on Enter, the item’s own secret, and goes away', async () => {
    const bridged = bridge()
    render(<Quick />)

    await userEvent.type(screen.getByRole('searchbox'), 'mail')
    await screen.findAllByRole('option')
    await userEvent.keyboard('{Enter}')

    await waitFor(() => expect(bridged.named('dismiss')).toHaveLength(1))
    expect(bridged.named('copy')).toEqual([['copy', { id: 2, field: null }]])
  })

  it('walks the list with the arrows', async () => {
    const bridged = bridge()
    render(<Quick />)

    await userEvent.type(screen.getByRole('searchbox'), 'mail')
    await screen.findAllByRole('option')
    await userEvent.keyboard('{ArrowDown}{Enter}')

    await waitFor(() => expect(bridged.named('copy')).toEqual([['copy', { id: 7, field: null }]]))
  })

  it('puts itself away on Escape without copying', async () => {
    const bridged = bridge()
    render(<Quick />)

    await userEvent.type(screen.getByRole('searchbox'), 'mail')
    await screen.findAllByRole('option')
    await userEvent.keyboard('{Escape}')

    await waitFor(() => expect(bridged.named('dismiss')).toHaveLength(1))
    expect(bridged.named('copy')).toHaveLength(0)
  })

  it('starts from an empty line each time it is brought up', async () => {
    bridge()
    render(<Quick />)

    await userEvent.type(screen.getByRole('searchbox'), 'mail')
    await screen.findAllByRole('option')
    // The mocks deliver an event to every listener; the process sends this
    // one to the quick window alone.
    await emit(SHOWN_EVENT)

    await waitFor(() => expect(screen.getByRole('searchbox')).toHaveValue(''))
    expect(screen.queryAllByRole('option')).toHaveLength(0)
  })

  it('stays, and says why, when there is nothing to copy', async () => {
    const bridged = bridge()
    render(<Quick />)

    await userEvent.type(screen.getByRole('searchbox'), 'scan')
    await screen.findAllByRole('option')
    await userEvent.keyboard('{Enter}')

    expect(await screen.findByRole('alert')).toHaveTextContent('is a file')
    expect(bridged.named('dismiss')).toHaveLength(0)
  })
})
