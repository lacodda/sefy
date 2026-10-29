import { act, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { mockIPC, mockWindows } from '@tauri-apps/api/mocks'
import { describe, expect, it, vi } from 'vitest'
import type { Filter, ItemCard, ItemRow, VaultStatus } from '@/ipc'
import { Vault } from '@/screens/Vault'

/*
 * The open vault, against a bridge that answers the way the process does:
 * rows and cards with every secret named and none filled in, a value only
 * when one is asked to be revealed, and nothing but a name and a number of
 * seconds back from a copy.
 */

const PASSWORD = 'Tq7#vmRl2!ax-not-real'
const CODE = '493201'

const STATUS: VaultStatus = {
  name: 'notes.bak',
  path: 'C:\\Backups\\notes.bak',
  size: 86_118,
  items: 3,
  kinds: [
    { kind: 'login', count: 2 },
    { kind: 'note', count: 1 },
  ],
  tags: 2,
  schema: 5,
  writtenByNewer: false,
  lastSync: null,
  copies: 0,
  newestCopy: null,
  sealing: {
    format: 1,
    kdf: 'Argon2id',
    memoryKib: 65536,
    passes: 3,
    lanes: 4,
    cipher: 'XChaCha20-Poly1305',
    saltLen: 16,
    nonceLen: 24,
    tagLen: 16,
  },
}

const ROWS: ItemRow[] = [
  { id: 1, title: 'bank · main', kind: 'login', known: true, tags: ['finance'], updatedAt: 1_700_000_000 },
  { id: 2, title: 'mailbox', kind: 'login', known: true, tags: ['work'], updatedAt: 1_690_000_000 },
  { id: 3, title: 'recovery codes', kind: 'note', known: true, tags: [], updatedAt: 1_680_000_000 },
]

const CARDS: Record<number, ItemCard> = {
  1: {
    id: 1,
    uuid: '7c9e6679-7425-40de-944b-e07fc1f90ae7',
    title: 'bank · main',
    kind: 'login',
    tags: ['finance'],
    createdAt: 1_600_000_000,
    updatedAt: 1_700_000_000,
    earlierVersions: 2,
    body: {
      shape: 'record',
      fields: [
        { shape: 'plain', name: 'login', value: 'k.miller', daysLeft: null },
        { shape: 'secret', name: 'password' },
        { shape: 'plain', name: 'url', value: 'https://bank.example.com', daysLeft: null },
        { shape: 'code', name: 'totp', period: 30 },
      ],
    },
  },
  2: {
    id: 2,
    uuid: '9b2f11ce-1c2d-4e5f-8a90-b1c2d3e4a801',
    title: 'mailbox',
    kind: 'login',
    tags: ['work'],
    createdAt: 1_600_000_000,
    updatedAt: 1_690_000_000,
    earlierVersions: 0,
    body: {
      shape: 'record',
      fields: [
        { shape: 'plain', name: 'login', value: 'kmiller-dev', daysLeft: null },
        { shape: 'secret', name: 'password' },
      ],
    },
  },
  3: {
    id: 3,
    uuid: '41d8cd98-f00b-4204-9800-998ecf8477e0',
    title: 'recovery codes',
    kind: 'note',
    tags: [],
    createdAt: 1_600_000_000,
    updatedAt: 1_680_000_000,
    earlierVersions: 0,
    body: { shape: 'note', text: 'mail: F7Q2-XXXX' },
  },
}

interface Bridge {
  calls: [string, Record<string, unknown> | undefined][]
  filters: () => Filter[]
}

function bridge(): Bridge {
  const state: Bridge = {
    calls: [],
    filters: () => state.calls.filter(([cmd]) => cmd === 'items').map(([, args]) => args!.filter as Filter),
  }
  mockWindows('main')
  mockIPC((cmd, args) => {
    const given = args as Record<string, unknown> | undefined
    state.calls.push([cmd, given])
    switch (cmd) {
      case 'items': {
        const filter = given!.filter as Filter
        return ROWS.filter(
          (row) =>
            (filter.text === null || row.title.includes(filter.text)) &&
            (filter.kind === null || row.kind === filter.kind) &&
            (filter.tag === null || row.tags.includes(filter.tag)),
        )
      }
      case 'tags':
        return [
          { tag: 'finance', count: 1 },
          { tag: 'work', count: 1 },
        ]
      case 'policy':
        return { clipboardSeconds: 45, quick: { keys: 'Ctrl+Alt+Space', active: true } }
      case 'item':
        return CARDS[given!.id as number]
      case 'reveal':
        return { value: given!.field === 'totp' ? CODE : PASSWORD }
      case 'copy':
        return { what: (given!.field as string | null) ?? 'text', seconds: 45 }
      default:
        return null
    }
  })
  return state
}

/** The open item's pane, found by its heading. */
async function paneOf(title: string) {
  const heading = await screen.findByRole('heading', { name: title })
  return heading.closest<HTMLElement>('[data-pane]')!
}

async function openBank() {
  await userEvent.click(await screen.findByRole('button', { name: /bank · main/ }))
  return paneOf('bank · main')
}

describe('the open vault', () => {
  it('lists what the core finds, and asks again with what is typed', async () => {
    const calls = bridge()
    render(<Vault status={STATUS} onLock={() => undefined} />)

    expect(await screen.findByRole('button', { name: /mailbox/ })).toBeInTheDocument()
    await userEvent.type(screen.getByRole('searchbox', { name: /Search/ }), 'mail')

    await waitFor(() => expect(screen.queryByRole('button', { name: /bank · main/ })).toBeNull())
    expect(calls.filters().at(-1)).toEqual({ text: 'mail', kind: null, tag: null })
  })

  it('narrows to a kind from the rail, and to a tag', async () => {
    const calls = bridge()
    render(<Vault status={STATUS} onLock={() => undefined} />)
    await screen.findByRole('button', { name: /mailbox/ })

    await userEvent.click(screen.getByRole('button', { name: /Notes/ }))
    await waitFor(() => expect(calls.filters().at(-1)).toEqual({ text: null, kind: 'note', tag: null }))

    await userEvent.click(screen.getByRole('button', { name: /All items/ }))
    // The chip, whose name starts with the tag; a row's starts with its kind.
    await userEvent.click(screen.getByRole('button', { name: /^work/ }))
    await waitFor(() => expect(calls.filters().at(-1)).toEqual({ text: null, kind: null, tag: 'work' }))
  })

  it('opens an item with every secret named and none on screen', async () => {
    bridge()
    render(<Vault status={STATUS} onLock={() => undefined} />)
    const card = await openBank()

    expect(within(card).getByText('k.miller')).toBeInTheDocument()
    expect(within(card).getByRole('region', { name: 'password' })).toHaveTextContent('••••')
    expect(document.body.textContent).not.toContain(PASSWORD)
    expect(document.body.textContent).not.toContain(CODE)
    expect(within(card).getByText(/2 earlier versions/)).toBeInTheDocument()
  })

  it('shows a secret only when asked, and hides it again', async () => {
    const calls = bridge()
    render(<Vault status={STATUS} onLock={() => undefined} />)
    const card = await openBank()

    const show = within(card).getByRole('button', { name: 'Show password' })
    await userEvent.click(show)
    expect(await within(card).findByText(PASSWORD)).toBeInTheDocument()
    expect(calls.calls.filter(([cmd]) => cmd === 'reveal')).toEqual([['reveal', { id: 1, field: 'password' }]])

    await userEvent.click(within(card).getByRole('button', { name: 'Hide password' }))
    expect(document.body.textContent).not.toContain(PASSWORD)
  })

  it('forgets a shown secret when another item is opened', async () => {
    bridge()
    render(<Vault status={STATUS} onLock={() => undefined} />)
    const card = await openBank()
    await userEvent.click(within(card).getByRole('button', { name: 'Show password' }))
    await within(card).findByText(PASSWORD)

    // Another login: the same fields in the same places, which is exactly
    // where a card drawn over the last one would carry the shown value along.
    await userEvent.click(screen.getByRole('button', { name: /mailbox/ }))
    const other = await paneOf('mailbox')
    expect(within(other).getByText('kmiller-dev')).toBeInTheDocument()
    expect(within(other).getByRole('region', { name: 'password' })).toHaveTextContent('••••')
    expect(document.body.textContent).not.toContain(PASSWORD)
  })

  it('copies through the process and counts the hold down on that button alone', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true })
    try {
      const calls = bridge()
      render(<Vault status={STATUS} onLock={() => undefined} />)
      const card = await openBank()

      await userEvent.click(within(card).getByRole('button', { name: 'Copy password' }))
      expect(calls.calls.filter(([cmd]) => cmd === 'copy')).toEqual([['copy', { id: 1, field: 'password' }]])
      const counting = await within(card).findByRole('button', { name: 'Copy password' })
      await waitFor(() => expect(counting).toHaveTextContent('✓ 45 s'))
      expect(within(card).getByRole('button', { name: 'Copy login' })).toHaveTextContent('Copy')
      // The value never came back to the page.
      expect(document.body.textContent).not.toContain(PASSWORD)

      await act(async () => vi.advanceTimersByTimeAsync(10_000))
      expect(counting).toHaveTextContent('✓ 35 s')
      await act(async () => vi.advanceTimersByTimeAsync(36_000))
      expect(counting).toHaveTextContent('Copy')
    } finally {
      vi.useRealTimers()
    }
  })

  it('copies the one-time code, not the key, and shows it only when asked', async () => {
    const calls = bridge()
    render(<Vault status={STATUS} onLock={() => undefined} />)
    const card = await openBank()
    const totp = within(card).getByRole('region', { name: 'totp' })

    expect(totp).toHaveTextContent('••• •••')
    await userEvent.click(within(totp).getByRole('button', { name: 'Copy one-time code' }))
    expect(calls.calls.filter(([cmd]) => cmd === 'copy').at(-1)).toEqual(['copy', { id: 1, field: 'totp' }])

    await userEvent.click(within(totp).getByRole('button', { name: 'Show one-time code' }))
    expect(await within(totp).findByText('493 201')).toBeInTheDocument()
  })

  it('closes the card when the list no longer holds the item', async () => {
    bridge()
    render(<Vault status={STATUS} onLock={() => undefined} />)
    await openBank()

    await userEvent.type(screen.getByRole('searchbox', { name: /Search/ }), 'mail')
    await waitFor(() => expect(screen.queryByRole('heading', { name: 'bank · main' })).toBeNull())
    expect(screen.getByText('Nothing open.')).toBeInTheDocument()
  })

  it('says how long a copy is held and which keys bring up quick search', async () => {
    bridge()
    render(<Vault status={STATUS} onLock={() => undefined} />)
    expect(await screen.findByText('the clipboard clears 45 s after a copy')).toBeInTheDocument()
    expect(screen.getByText('quick search: Ctrl+Alt+Space')).toBeInTheDocument()
  })
})
