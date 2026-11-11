import { act, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { mockIPC, mockWindows } from '@tauri-apps/api/mocks'
import { describe, expect, it, vi } from 'vitest'
import type { Draft, Edit, Filter, Forms, ItemCard, ItemRow, VaultStatus, VersionRow } from '@/ipc'
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

const HISTORY: VersionRow[] = [
  {
    number: 1,
    uuid: 'first-version',
    madeAt: 1_600_000_000,
    device: null,
    conflict: false,
    current: false,
    changed: [],
    body: {
      shape: 'record',
      fields: [
        { shape: 'plain', name: 'login', value: 'k.miller.old', daysLeft: null },
        { shape: 'secret', name: 'password' },
      ],
    },
  },
  {
    number: 2,
    uuid: 'other-machines',
    madeAt: 1_650_000_000,
    device: 'laptop',
    conflict: true,
    current: false,
    changed: ['password'],
    body: {
      shape: 'record',
      fields: [
        { shape: 'plain', name: 'login', value: 'k.miller.laptop', daysLeft: null },
        { shape: 'secret', name: 'password' },
      ],
    },
  },
  {
    number: 3,
    uuid: 'now',
    madeAt: 1_700_000_000,
    device: 'desk',
    conflict: false,
    current: true,
    changed: ['password', 'url'],
    body: {
      shape: 'record',
      fields: [
        { shape: 'plain', name: 'login', value: 'k.miller', daysLeft: null },
        { shape: 'secret', name: 'password' },
      ],
    },
  },
]

/** What the process says the editor can make: the core's templates, in part. */
const FORMS: Forms = {
  kinds: [
    { kind: 'note', shape: 'note', fields: [] },
    {
      kind: 'login',
      shape: 'record',
      fields: [
        { name: 'login', hint: 'username, email, or whatever the service calls it', secret: false, multiline: false, generated: false },
        { name: 'password', hint: 'the secret itself', secret: true, multiline: false, generated: true },
        { name: 'url', hint: 'where the account lives', secret: false, multiline: false, generated: false },
        { name: 'totp', hint: 'key for one-time passwords', secret: true, multiline: false, generated: false },
        { name: 'notes', hint: 'anything else worth remembering', secret: false, multiline: true, generated: false },
      ],
    },
    { kind: 'file', shape: 'file', fields: [] },
  ],
  recipe: { make: 'characters', length: 20, uppercase: true, digits: true, symbols: true },
  words: 6,
  maxLength: 1024,
}

interface Bridge {
  calls: [string, Record<string, unknown> | undefined][]
  filters: () => Filter[]
  /** The calls of one command, by their arguments. */
  of: (cmd: string) => (Record<string, unknown> | undefined)[]
}

function bridge({ rows = ROWS }: { rows?: ItemRow[] } = {}): Bridge {
  const state: Bridge = {
    calls: [],
    filters: () => state.calls.filter(([cmd]) => cmd === 'items').map(([, args]) => args!.filter as Filter),
    of: (cmd) => state.calls.filter(([called]) => called === cmd).map(([, args]) => args),
  }
  mockWindows('main')
  mockIPC((cmd, args) => {
    const given = args as Record<string, unknown> | undefined
    state.calls.push([cmd, given])
    switch (cmd) {
      case 'forms':
        return FORMS
      case 'worth':
        return { bits: 131.1, offline: true }
      case 'add_item':
        return 4
      case 'edit_item':
        return true
      case 'delete_item':
        return true
      case 'items': {
        const filter = given!.filter as Filter
        return rows.filter(
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
        return {
          clipboardSeconds: 45,
          lockAfterSeconds: 900,
          clipboardChoices: [15, 30, 45, 90, 180],
          lockAfterChoices: [60, 300, 900, 1800, 3600, 14_400],
          quick: { keys: 'Ctrl+Alt+Space', active: true },
        }
      case 'history':
        return HISTORY
      case 'restore':
        return true
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

  describe('its history', () => {
    it('lists the versions newest first, and opens the one a conflict left', async () => {
      bridge()
      render(<Vault status={STATUS} onLock={() => undefined} />)
      const card = await openBank()
      await userEvent.click(await within(card).findByRole('button', { name: /History: 2 earlier versions/ }))

      const history = await within(card).findByRole('region', { name: 'History' })
      const rows = (await within(history).findAllByRole('button', { name: /^v\d/ })).map((row) => row.textContent)
      expect(rows[0]).toMatch(/^v3/)
      expect(rows[0]).toContain('current')
      expect(rows[1]).toMatch(/^v2/)
      expect(rows[1]).toContain('laptop')
      expect(rows[1]).toContain('kept from a conflict')
      expect(rows[2]).toMatch(/^v1/)

      // The conflict's version is open, and no other.
      expect(within(history).getByText('k.miller.laptop')).toBeInTheDocument()
      expect(within(history).queryByText('k.miller.old')).not.toBeInTheDocument()
    })

    it('asks for one secret of one version, only when asked', async () => {
      const calls = bridge()
      render(<Vault status={STATUS} onLock={() => undefined} />)
      const card = await openBank()
      await userEvent.click(await within(card).findByRole('button', { name: /History/ }))
      const history = await within(card).findByRole('region', { name: 'History' })
      await within(history).findByText('k.miller.laptop')

      const secrets = () => calls.calls.filter(([cmd]) => cmd === 'reveal')
      expect(secrets()).toEqual([])
      await userEvent.click(within(history).getByRole('button', { name: 'Show password' }))
      await waitFor(() =>
        expect(secrets()).toEqual([['reveal', { id: 1, field: 'password', version: 'other-machines' }]]),
      )

      await userEvent.click(within(history).getByRole('button', { name: 'Copy password' }))
      await waitFor(() =>
        expect(calls.calls.filter(([cmd]) => cmd === 'copy')).toEqual([
          ['copy', { id: 1, field: 'password', version: 'other-machines' }],
        ]),
      )
    })

    it('brings a version back, and says what became of the one it replaced', async () => {
      const calls = bridge()
      render(<Vault status={STATUS} onLock={() => undefined} />)
      const card = await openBank()
      await userEvent.click(await within(card).findByRole('button', { name: /History/ }))
      const history = await within(card).findByRole('region', { name: 'History' })
      await within(history).findByText('k.miller.laptop')

      await userEvent.click(within(history).getByRole('button', { name: 'Bring back version 2' }))
      await waitFor(() =>
        expect(calls.calls.filter(([cmd]) => cmd === 'restore')).toEqual([
          ['restore', { id: 1, version: 'other-machines' }],
        ]),
      )
      expect(await within(history).findByRole('status')).toHaveTextContent('Version 2 is back')
    })

    it('offers no way to bring back what is already current', async () => {
      bridge()
      render(<Vault status={STATUS} onLock={() => undefined} />)
      const card = await openBank()
      await userEvent.click(await within(card).findByRole('button', { name: /History/ }))
      const history = await within(card).findByRole('region', { name: 'History' })
      await userEvent.click(await within(history).findByRole('button', { name: /^v3/ }))
      expect(within(history).queryByRole('button', { name: /Bring back/ })).not.toBeInTheDocument()
    })

    it('goes from a conflict on the Sync screen to the item, with its history showing', async () => {
      const calls = bridge()
      const answer = (cmd: string) =>
        cmd === 'transports'
          ? { found: [], directory: null, remote: 'vault' }
          : cmd === 'last_transfer'
            ? {
                at: 1_700_000_100,
                transport: 'relay',
                operation: 'sync',
                added: [],
                updated: [],
                conflicts: [{ id: 1, title: 'bank · main', otherIsCurrent: true }],
                unchanged: 0,
                versions: 0,
                unsupported: 0,
                keptCopy: null,
                said: [],
                sendFailure: null,
              }
            : undefined
      const given = calls.calls
      mockIPC((cmd, args) => {
        given.push([cmd, args as Record<string, unknown> | undefined])
        const special = answer(cmd)
        if (special !== undefined) return special
        switch (cmd) {
          case 'items':
            return ROWS
          case 'tags':
            return []
          case 'policy':
            return null
          case 'item':
            return CARDS[1]
          case 'history':
            return HISTORY
          default:
            return null
        }
      })
      render(<Vault status={STATUS} onLock={() => undefined} />)
      await userEvent.click(await screen.findByRole('button', { name: 'Sync' }))
      await userEvent.click(await screen.findByRole('button', { name: 'bank · main' }))

      const heading = await screen.findByRole('heading', { name: 'bank · main' })
      const card = heading.closest<HTMLElement>('[data-pane]')!
      expect(await within(card).findByRole('region', { name: 'History' })).toBeInTheDocument()
      expect(await within(card).findByText('k.miller.laptop')).toBeInTheDocument()
    })
  })

  describe('editing', () => {
    const editor = () => screen.findByRole('form', { name: /New item|Edit/ })

    it('sends an empty vault to New item, not to the command line', async () => {
      bridge({ rows: [] })
      render(<Vault status={{ ...STATUS, items: 0, kinds: [] }} onLock={() => undefined} />)
      expect(await screen.findByText('Nothing in this vault yet.')).toBeInTheDocument()
      expect(document.body.textContent).not.toContain('sefy add')
      const add = screen.getAllByRole('button', { name: 'New item' }).at(-1)!
      await waitFor(() => expect(add).toBeEnabled())
      await userEvent.click(add)
      expect(await editor()).toBeInTheDocument()
    })

    it('makes a login of what was typed, and sends what was left empty nowhere', async () => {
      const calls = bridge()
      render(<Vault status={STATUS} onLock={() => undefined} />)
      const add = screen.getByRole('button', { name: 'New item' })
      await waitFor(() => expect(add).toBeEnabled())
      await userEvent.click(add)
      const form = await editor()

      await userEvent.type(within(form).getByLabelText(/^Title/), 'mail')
      await userEvent.type(within(form).getByLabelText('login', { selector: 'input' }), 'kmiller')
      await userEvent.type(within(form).getByLabelText('password', { selector: 'input' }), 'typed-not-real')
      await userEvent.type(within(form).getByLabelText(/^Tags/), 'work{Enter}')
      await userEvent.click(within(form).getByRole('button', { name: 'Save' }))

      await waitFor(() => expect(calls.of('add_item')).toHaveLength(1))
      const draft = calls.of('add_item')[0]!.draft as Draft
      expect(draft).toEqual({
        title: 'mail',
        tags: ['work'],
        contents: {
          shape: 'record',
          kind: 'login',
          fields: [
            { name: 'login', secret: null, value: { from: 'typed', text: 'kmiller' } },
            { name: 'password', secret: null, value: { from: 'typed', text: 'typed-not-real' } },
          ],
        },
      })
      await waitFor(() => expect(screen.queryByRole('form', { name: /New item/ })).toBeNull())
    })

    it('generates a password in the process: the page sends the recipe and gets no value', async () => {
      const calls = bridge()
      render(<Vault status={STATUS} onLock={() => undefined} />)
      const add = screen.getByRole('button', { name: 'New item' })
      await waitFor(() => expect(add).toBeEnabled())
      await userEvent.click(add)
      const form = await editor()
      await userEvent.type(within(form).getByLabelText(/^Title/), 'bank')
      await userEvent.click(within(form).getByRole('button', { name: 'Generate password' }))

      expect(await within(form).findByText(/131 bits of entropy/)).toBeInTheDocument()
      expect(calls.of('worth')[0]).toEqual({ recipe: FORMS.recipe })
      await userEvent.click(within(form).getByRole('button', { name: 'Save' }))

      await waitFor(() => expect(calls.of('add_item')).toHaveLength(1))
      const draft = calls.of('add_item')[0]!.draft as Draft
      expect(draft.contents).toEqual({
        shape: 'record',
        kind: 'login',
        fields: [{ name: 'password', secret: null, value: { from: 'generated', recipe: FORMS.recipe } }],
      })
    })

    it('starts on the kind the rail shows', async () => {
      bridge()
      render(<Vault status={STATUS} onLock={() => undefined} />)
      await screen.findByRole('button', { name: /mailbox/ })
      await userEvent.click(screen.getByRole('button', { name: /Notes/ }))
      await userEvent.click(screen.getByRole('button', { name: 'New item' }))
      const form = await editor()
      expect(within(form).getByLabelText('Note', { selector: 'textarea' })).toBeInTheDocument()
      expect(within(form).queryByLabelText('password', { selector: 'input' })).toBeNull()
    })

    it('edits what changed and nothing else, without asking for a secret to edit it', async () => {
      const calls = bridge()
      render(<Vault status={STATUS} onLock={() => undefined} />)
      const card = await openBank()
      await userEvent.click(within(card).getByRole('button', { name: 'Edit' }))
      const form = await editor()

      // A secret is named and kept; its value is not on the page to edit.
      expect(within(form).getByRole('region', { name: 'password' })).toHaveTextContent('unchanged')
      expect(within(form).getByLabelText('login', { selector: 'input' })).toHaveValue('k.miller')
      const url = within(form).getByLabelText('url', { selector: 'input' })
      await userEvent.clear(url)
      await userEvent.type(url, 'https://bank.example.org')
      await userEvent.click(within(form).getByRole('button', { name: 'Save' }))

      await waitFor(() => expect(calls.of('edit_item')).toHaveLength(1))
      expect(calls.of('edit_item')[0]).toEqual({
        uuid: CARDS[1]!.uuid,
        edit: {
          title: null,
          tags: null,
          text: null,
          fields: [{ change: 'set', name: 'url', secret: null, value: { from: 'typed', text: 'https://bank.example.org' } }],
        } satisfies Edit,
      })
      expect(calls.of('reveal')).toEqual([])
      expect(document.body.textContent).not.toContain(PASSWORD)
    })

    it('rotates a password in one gesture, and takes away a key when asked', async () => {
      const calls = bridge()
      render(<Vault status={STATUS} onLock={() => undefined} />)
      const card = await openBank()
      await userEvent.click(within(card).getByRole('button', { name: 'Edit' }))
      const form = await editor()

      // A key is issued by the site: there is nothing to generate it from. Asked
      // while it is kept, which is when the other buttons are offered.
      expect(within(form).getByRole('button', { name: 'Change totp' })).toBeInTheDocument()
      expect(within(form).queryByRole('button', { name: 'Generate totp' })).toBeNull()

      await userEvent.click(within(form).getByRole('button', { name: 'Generate password' }))
      await userEvent.click(within(form).getByRole('button', { name: 'Remove totp' }))
      expect(within(form).getByRole('region', { name: 'totp' })).toHaveTextContent('removed when you save')
      await userEvent.click(within(form).getByRole('button', { name: 'Save' }))

      await waitFor(() => expect(calls.of('edit_item')).toHaveLength(1))
      expect((calls.of('edit_item')[0]!.edit as Edit).fields).toEqual([
        { change: 'set', name: 'password', secret: null, value: { from: 'generated', recipe: FORMS.recipe } },
        { change: 'remove', name: 'totp' },
      ])
    })

    it('saves nothing when nothing changed', async () => {
      const calls = bridge()
      render(<Vault status={STATUS} onLock={() => undefined} />)
      const card = await openBank()
      await userEvent.click(within(card).getByRole('button', { name: 'Edit' }))
      const form = await editor()
      await userEvent.click(within(form).getByRole('button', { name: 'Save' }))
      await waitFor(() => expect(screen.queryByRole('form', { name: /Edit/ })).toBeNull())
      expect(calls.of('edit_item')).toEqual([])
    })

    it('asks before what was typed goes, and keeps it when told to', async () => {
      const calls = bridge()
      render(<Vault status={STATUS} onLock={() => undefined} />)
      const add = screen.getByRole('button', { name: 'New item' })
      await waitFor(() => expect(add).toBeEnabled())
      await userEvent.click(add)
      const form = await editor()
      await userEvent.type(within(form).getByLabelText(/^Title/), 'half typed')

      await userEvent.click(screen.getByRole('button', { name: /mailbox/ }))
      const question = await screen.findByRole('alertdialog', { name: 'Leave without saving?' })
      await userEvent.click(within(question).getByRole('button', { name: 'Keep editing' }))
      await waitFor(() => expect(screen.queryByRole('alertdialog')).toBeNull())
      expect(within(await editor()).getByLabelText(/^Title/)).toHaveValue('half typed')

      await userEvent.click(screen.getByRole('button', { name: /mailbox/ }))
      await userEvent.click(within(await screen.findByRole('alertdialog')).getByRole('button', { name: 'Leave' }))
      expect(await paneOf('mailbox')).toBeInTheDocument()
      expect(calls.of('add_item')).toEqual([])
    })

    it('deletes after asking, and says what goes with the item', async () => {
      const calls = bridge()
      render(<Vault status={STATUS} onLock={() => undefined} />)
      const card = await openBank()
      await userEvent.click(within(card).getByRole('button', { name: 'Delete' }))

      const question = await screen.findByRole('alertdialog', { name: 'Delete bank · main?' })
      expect(question).toHaveTextContent('Its 2 earlier versions go with it')
      expect(question).toHaveTextContent('a sync brings it back')
      await userEvent.click(within(question).getByRole('button', { name: 'Delete it' }))

      await waitFor(() => expect(calls.of('delete_item')).toEqual([{ uuid: CARDS[1]!.uuid }]))
      await waitFor(() => expect(screen.queryByRole('heading', { name: 'bank · main' })).toBeNull())
    })
  })
})
