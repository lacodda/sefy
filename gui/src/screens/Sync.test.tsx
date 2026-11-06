import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { mockIPC, mockWindows } from '@tauri-apps/api/mocks'
import { describe, expect, it, vi } from 'vitest'
import type { Failure, Transfer, Transports, VaultStatus } from '@/ipc'
import { Sync } from '@/screens/Sync'

/*
 * The transports, against a bridge that answers as the process does: every
 * transport listed whether or not it works, and what a sync did, by title.
 */

const STATUS: VaultStatus = {
  name: 'notes.bak',
  path: 'C:\\Backups\\notes.bak',
  size: 86_118,
  items: 3,
  kinds: [],
  tags: 0,
  schema: 5,
  writtenByNewer: false,
  lastSync: { at: 1_700_000_000, transport: 'relay', operation: 'sync' },
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

const TRANSPORTS: Transports = {
  found: [
    { name: 'relay', version: '0.20.0', protocol: 1, operations: ['push', 'pull'], problem: null },
    { name: 'publish', version: '1.0.0', protocol: 1, operations: ['push'], problem: null },
    { name: 'future', version: '9.0.0', protocol: 99, operations: [], problem: 'it speaks protocol 99 and this build speaks 1' },
  ],
  directory: 'C:\\Users\\you\\AppData\\Roaming\\sefy\\plugins',
  remote: 'vault',
}

const MERGED: Transfer = {
  at: 1_700_000_100,
  transport: 'relay',
  operation: 'sync',
  added: [{ id: 7, title: 'vpn office' }],
  updated: [{ id: 2, title: 'apartment wifi' }],
  conflicts: [{ id: 4, title: 'bank · main', otherIsCurrent: true }],
  unchanged: 14,
  versions: 2,
  unsupported: 0,
  keptCopy: 'C:\\Backups\\.notes.bak.1',
  said: ['fetched "vault" (52.1 KiB)'],
  sendFailure: null,
}

interface Bridge {
  calls: [string, Record<string, unknown> | undefined][]
  answer: (cmd: string, args: Record<string, unknown> | undefined) => unknown
}

function bridge(sync: (password: string | null) => Transfer | Failure, last: Transfer | null = null): Bridge {
  const state: Bridge = {
    calls: [],
    answer: (cmd, args) => {
      switch (cmd) {
        case 'transports':
          return TRANSPORTS
        case 'last_transfer':
          return last
        case 'sync_now': {
          const answer = sync((args?.remotePassword as string | null) ?? null)
          if ('kind' in answer) throw answer
          return answer
        }
        case 'push_now':
          return { ...MERGED, operation: 'push', added: [], updated: [], conflicts: [], keptCopy: null, said: [] }
        default:
          return null
      }
    },
  }
  mockWindows('main')
  mockIPC((cmd, args) => {
    const given = args as Record<string, unknown> | undefined
    state.calls.push([cmd, given])
    return state.answer(cmd, given)
  })
  return state
}

const card = (name: string) => screen.findByRole('group', { name })

describe('the Sync screen', () => {
  it('lists every transport, and says why the one that cannot be used cannot', async () => {
    bridge(() => MERGED)
    render(<Sync status={STATUS} onOpenHistory={() => undefined} />)

    const relay = await card('relay')
    expect(within(relay).getByText('last used by this vault')).toBeInTheDocument()
    expect(within(relay).getByRole('button', { name: 'Sync now' })).toBeEnabled()

    const future = await card('future')
    expect(within(future).getByText(/speaks protocol 99/)).toBeInTheDocument()
    expect(within(future).getByRole('button', { name: 'Sync now' })).toBeDisabled()
    expect(within(future).getByRole('button', { name: 'Push…' })).toBeDisabled()

    // A transport that can only publish can push and cannot sync.
    const publish = await card('publish')
    expect(within(publish).getByRole('button', { name: 'Sync now' })).toBeDisabled()
    expect(within(publish).getByRole('button', { name: 'Push…' })).toBeEnabled()

    expect(screen.getByText(TRANSPORTS.directory!)).toBeInTheDocument()
  })

  it('says what a sync brought, and takes a conflict to the item', async () => {
    const calls = bridge(() => MERGED)
    const onOpenHistory = vi.fn()
    render(<Sync status={STATUS} onOpenHistory={onOpenHistory} />)

    await userEvent.click(within(await card('relay')).getByRole('button', { name: 'Sync now' }))

    expect(await screen.findByText('vpn office')).toBeInTheDocument()
    expect(screen.getByText('apartment wifi')).toBeInTheDocument()
    expect(screen.getByText('kept both')).toBeInTheDocument()
    expect(screen.getByText(/Both versions stay in the vault/)).toBeInTheDocument()
    expect(screen.getByText(/14 unchanged/)).toBeInTheDocument()
    expect(screen.getByText(/\.notes\.bak\.1/)).toBeInTheDocument()

    const syncs = calls.calls.filter(([cmd]) => cmd === 'sync_now')
    expect(syncs).toEqual([['sync_now', { transport: 'relay', remotePassword: null }]])

    await userEvent.click(screen.getByRole('button', { name: 'bank · main' }))
    expect(onOpenHistory).toHaveBeenCalledWith(4)
  })

  it('shows what the last sync did on coming back to the screen', async () => {
    bridge(() => MERGED, MERGED)
    render(<Sync status={STATUS} onOpenHistory={() => undefined} />)
    expect(await screen.findByText('vpn office')).toBeInTheDocument()
  })

  it('picks up a sync that finished while the screen was elsewhere', async () => {
    const state = bridge(() => MERGED, null)
    const { rerender } = render(<Sync status={STATUS} revision={1} onOpenHistory={() => undefined} />)
    expect(await screen.findByText(/It last reached a remote/)).toBeInTheDocument()

    const answer = state.answer
    state.answer = (cmd, args) => (cmd === 'last_transfer' ? MERGED : answer(cmd, args))
    rerender(<Sync status={STATUS} revision={2} onOpenHistory={() => undefined} />)
    expect(await screen.findByText('vpn office')).toBeInTheDocument()
  })

  it('asks for the copy’s own password when this vault’s does not open it', async () => {
    const refused: Failure = { kind: 'remote-refused', message: 'that password does not open the copy at relay' }
    const calls = bridge((password) => (password === 'the remote one' ? MERGED : refused))
    render(<Sync status={STATUS} onOpenHistory={() => undefined} />)

    await userEvent.click(within(await card('relay')).getByRole('button', { name: 'Sync now' }))

    const dialog = await screen.findByRole('dialog')
    expect(within(dialog).getByText(/does not open the copy kept through relay/)).toBeInTheDocument()
    // Not said as a failure before anyone has typed anything.
    expect(within(dialog).queryByRole('alert')).not.toBeInTheDocument()

    // A wrong one says so, and stays.
    await userEvent.type(within(dialog).getByLabelText('Password of the remote copy'), 'wrong')
    await userEvent.click(within(dialog).getByRole('button', { name: 'Sync now' }))
    expect(await within(dialog).findByRole('alert')).toHaveTextContent('does not open the copy')

    const field = within(dialog).getByLabelText('Password of the remote copy')
    await userEvent.clear(field)
    await userEvent.type(field, 'the remote one')
    await userEvent.click(within(dialog).getByRole('button', { name: 'Sync now' }))

    expect(await screen.findByText('vpn office')).toBeInTheDocument()
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
    expect(calls.calls.filter(([cmd]) => cmd === 'sync_now').at(-1)).toEqual([
      'sync_now',
      { transport: 'relay', remotePassword: 'the remote one' },
    ])
  })

  it('says it plainly when a sync fails', async () => {
    const failed: Failure = {
      kind: 'other',
      message: 'the repository holds no copy called "vault" yet',
    }
    bridge(() => failed)
    render(<Sync status={STATUS} onOpenHistory={() => undefined} />)

    await userEvent.click(within(await card('relay')).getByRole('button', { name: 'Sync now' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('holds no copy called')
  })

  it('says what is in the vault when the result cannot be sent back', async () => {
    bridge(() => ({ ...MERGED, sendFailure: 'the network went away' }))
    render(<Sync status={STATUS} onOpenHistory={() => undefined} />)

    await userEvent.click(within(await card('relay')).getByRole('button', { name: 'Sync now' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('What came down is in the vault')
    expect(screen.getByText('vpn office')).toBeInTheDocument()
  })

  it('does not replace the remote copy without being told twice', async () => {
    const calls = bridge(() => MERGED)
    render(<Sync status={STATUS} onOpenHistory={() => undefined} />)

    await userEvent.click(within(await card('relay')).getByRole('button', { name: 'Push…' }))
    const dialog = await screen.findByRole('alertdialog')
    expect(within(dialog).getByText(/in place of whatever the remote holds/)).toBeInTheDocument()
    expect(calls.calls.some(([cmd]) => cmd === 'push_now')).toBe(false)

    await userEvent.click(within(dialog).getByRole('button', { name: 'Cancel' }))
    await waitFor(() => expect(screen.queryByRole('alertdialog')).not.toBeInTheDocument())
    expect(calls.calls.some(([cmd]) => cmd === 'push_now')).toBe(false)

    await userEvent.click(within(await card('relay')).getByRole('button', { name: 'Push…' }))
    await userEvent.click(within(await screen.findByRole('alertdialog')).getByRole('button', { name: 'Push' }))
    expect(await screen.findByText('The copy at the remote is now this vault.')).toBeInTheDocument()
    expect(calls.calls.filter(([cmd]) => cmd === 'push_now')).toEqual([['push_now', { transport: 'relay' }]])
  })
})
