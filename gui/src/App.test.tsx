import { act, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { mockIPC, mockWindows } from '@tauri-apps/api/mocks'
import { beforeEach, describe, expect, it } from 'vitest'
import App from '@/App'
import type { Failure, VaultStatus } from '@/ipc'

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

const REFUSED: Failure = { kind: 'refused', message: 'wrong password or not a vault file' }

interface Backend {
  file: string | null
  /** Whether unlocking waits for the test to let go; otherwise it answers at once. */
  hold: boolean
  calls: [string, Record<string, unknown> | undefined][]
  /** Held until the test lets go, so the screen can be read mid-unlock. */
  release?: () => void
}

/** A bridge that opens `notes.bak` with the password `right` and nothing else. */
function backend(file: string | null = STATUS.path, hold = true): Backend {
  const state: Backend = { file, hold, calls: [] }
  mockWindows('main')
  mockIPC(async (cmd, args) => {
    state.calls.push([cmd, args as Record<string, unknown> | undefined])
    switch (cmd) {
      case 'opening':
        return { file: state.file }
      case 'unlock': {
        if (state.hold) await new Promise<void>((resolve) => (state.release = resolve))
        if ((args as { password: string }).password !== 'right') throw REFUSED
        return STATUS
      }
      case 'lock':
        return null
      default:
        return null
    }
  })
  return state
}

/** The static splash `index.html` paints, as the page finds it. */
function staticSplash() {
  const splash = document.createElement('div')
  splash.id = 'splash'
  document.body.appendChild(splash)
}

async function typePassword(password: string) {
  const field = await screen.findByLabelText('Master password')
  await userEvent.clear(field)
  await userEvent.type(field, password)
  await userEvent.click(screen.getByRole('button', { name: 'Unlock' }))
}

describe('the window', () => {
  beforeEach(() => {
    staticSplash()
  })

  it('takes the static splash down, then shows the Unlock screen without a sweep', async () => {
    backend()
    render(<App />)

    expect(await screen.findByRole('heading', { name: 'Open a vault' })).toBeInTheDocument()
    expect(document.getElementById('splash')).toBeNull()
    // Typing a password is not loading: nothing on this screen sweeps.
    expect(screen.queryByRole('status')).toBeNull()
    expect(document.querySelector('[data-sweep]')).toBeNull()
  })

  it('locks whatever an earlier page left open before it asks for a password', async () => {
    // After a reload the vault the last page unlocked is still held by the
    // process; a page that knows nothing of it must not leave it open.
    const bridge = backend()
    render(<App />)
    await screen.findByRole('heading', { name: 'Open a vault' })
    expect(bridge.calls.map(([cmd]) => cmd)).toContain('lock')
  })

  it('offers the file it was started on, and starts in the password', async () => {
    backend()
    render(<App />)

    const file = await screen.findByRole('group', { name: 'Vault file' })
    expect(file).toHaveTextContent('C:\\Backups\\notes.bak')
    await waitFor(() => expect(screen.getByLabelText('Master password')).toHaveFocus())
  })

  it('cannot be unlocked without a file and a password', async () => {
    backend(null)
    render(<App />)

    expect(await screen.findByText('No file chosen')).toBeInTheDocument()
    await userEvent.type(screen.getByLabelText('Master password'), 'right')
    expect(screen.getByRole('button', { name: 'Unlock' })).toBeDisabled()
  })

  it('shows the splash while unsealing, and the splash names no vault', async () => {
    const bridge = backend()
    render(<App />)
    await typePassword('right')

    const splash = await screen.findByRole('status')
    expect(splash).toHaveTextContent('Unsealing the vault')
    expect(splash.querySelector('[data-sweep="on"]')).not.toBeNull()
    for (const giveaway of ['notes.bak', 'Backups', 'items']) {
      expect(splash.textContent).not.toContain(giveaway)
    }
    // The form under it stands still and takes no input.
    expect(screen.getByRole('main', { hidden: true })).toHaveAttribute('inert')

    await act(async () => bridge.release?.())
    expect(await screen.findByRole('heading', { name: 'notes.bak' })).toBeInTheDocument()
    expect(screen.queryByRole('status')).toBeNull()
  })

  it('does not blink the splash when unsealing is quick', async () => {
    backend(STATUS.path, false)
    const seen: string[] = []
    const watch = new MutationObserver(() => {
      if (document.querySelector('[role=status]')) seen.push('splash')
    })
    render(<App />)
    await screen.findByRole('heading', { name: 'Open a vault' })
    watch.observe(document.body, { childList: true, subtree: true })
    await typePassword('right')

    expect(await screen.findByRole('heading', { name: 'notes.bak' })).toBeInTheDocument()
    watch.disconnect()
    expect(seen).toEqual([])
  })

  it('comes back to the same file after a refusal, with the password to type again', async () => {
    const bridge = backend()
    render(<App />)
    await typePassword('wrong')
    await act(async () => bridge.release?.())

    expect(await screen.findByRole('alert')).toHaveTextContent('Wrong password, or this file is not a vault')
    const field = screen.getByLabelText('Master password')
    expect(field).toHaveValue('')
    await waitFor(() => expect(field).toHaveFocus())
    expect(screen.getByRole('group', { name: 'Vault file' })).toHaveTextContent('notes.bak')
    // The password went to the bridge once and nowhere else.
    expect(bridge.calls.filter(([cmd]) => cmd === 'unlock')).toEqual([
      ['unlock', { path: STATUS.path, password: 'wrong' }],
    ])
  })

  it('describes the open vault in shape, and locks back to the same file', async () => {
    const bridge = backend()
    render(<App />)
    await typePassword('right')
    await act(async () => bridge.release?.())

    const contents = await screen.findByRole('heading', { name: 'notes.bak' })
    const main = contents.closest('main')!
    expect(within(main).getAllByText('3 items')).not.toHaveLength(0)
    expect(within(main).getByText('Argon2id (64 MiB, 3 passes, 4 lanes)')).toBeInTheDocument()
    expect(within(main).getByText('XChaCha20-Poly1305')).toBeInTheDocument()
    expect(within(main).getByText('never')).toBeInTheDocument()

    const locks = () => bridge.calls.filter(([cmd]) => cmd === 'lock').length
    const before = locks()
    await userEvent.click(screen.getByRole('button', { name: 'Lock' }))
    expect(await screen.findByRole('heading', { name: 'Open a vault' })).toBeInTheDocument()
    expect(screen.getByRole('group', { name: 'Vault file' })).toHaveTextContent('notes.bak')
    expect(locks()).toBe(before + 1)
  })
})
