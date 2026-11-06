import { act, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { emit } from '@tauri-apps/api/event'
import { mockIPC, mockWindows } from '@tauri-apps/api/mocks'
import { beforeEach, describe, expect, it } from 'vitest'
import App, { CHANGED_EVENT, HANDED_OVER_EVENT, LOCKED_EVENT, UNREADABLE_EVENT } from '@/App'
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
  /** A file a second launch left, until the page takes it. */
  handover: string | null
}

/** A bridge that opens `notes.bak` with the password `right` and nothing else. */
function backend(file: string | null = STATUS.path, hold = true): Backend {
  const state: Backend = { file, hold, calls: [], handover: null }
  mockWindows('main')
  mockIPC(
    async (cmd, args) => {
    state.calls.push([cmd, args as Record<string, unknown> | undefined])
    switch (cmd) {
      case 'opening':
        return { file: state.file }
      case 'take_handover': {
        const file = state.handover
        state.handover = null
        return file
      }
      case 'unlock': {
        if (state.hold) await new Promise<void>((resolve) => (state.release = resolve))
        if ((args as { password: string }).password !== 'right') throw REFUSED
        return STATUS
      }
      case 'lock':
        return null
      case 'items':
        return []
      case 'tags':
        return []
      case 'status':
        return STATUS
      case 'wipe_preview':
        return { name: STATUS.name, directory: 'C:\\Backups', parts: [], remote: 'github' }
      case 'wipe':
        return { destroyed: 2, vanished: 0, failed: [], remote: 'github' }
      case 'policy':
        return {
          clipboardSeconds: 45,
          lockAfterSeconds: 900,
          clipboardChoices: [15, 30, 45, 90, 180],
          lockAfterChoices: [60, 300, 900, 1800, 3600, 14_400],
          quick: { keys: 'Ctrl+Alt+Space', active: true },
        }
      default:
        return null
    }
    },
    { shouldMockEvents: true },
  )
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

  it('goes back to the Unlock screen on the same file when the vault locks without it', async () => {
    // The time ran out, or `sefy lock` came from a terminal.
    backend(STATUS.path, false)
    render(<App />)
    await typePassword('right')
    await screen.findByRole('heading', { name: 'notes.bak' })

    await act(async () => emit(LOCKED_EVENT))

    expect(await screen.findByRole('heading', { name: 'Open a vault' })).toBeInTheDocument()
    expect(screen.queryByRole('heading', { name: 'notes.bak' })).toBeNull()
    expect(screen.getByRole('group', { name: 'Vault file' })).toHaveTextContent('C:\\Backups\\notes.bak')
    expect(screen.getByLabelText('Master password')).toHaveValue('')
  })

  it('stays where it is if a lock it did not ask for finds nothing open', async () => {
    backend(null)
    render(<App />)
    await screen.findByRole('heading', { name: 'Open a vault' })
    await act(async () => emit(LOCKED_EVENT))
    expect(screen.getByRole('heading', { name: 'Open a vault' })).toBeInTheDocument()
    expect(screen.getByText('No file chosen')).toBeInTheDocument()
  })

  it('offers a file a second launch handed over', async () => {
    const bridge = backend(null)
    render(<App />)
    await screen.findByText('No file chosen')

    bridge.handover = 'D:\\Elsewhere\\archive.dat'
    await act(async () => emit(HANDED_OVER_EVENT))

    const file = await screen.findByRole('group', { name: 'Vault file' })
    await waitFor(() => expect(file).toHaveTextContent('archive.dat'))
    await waitFor(() => expect(screen.getByLabelText('Master password')).toHaveFocus())
  })

  it('takes a file handed over before it was listening', async () => {
    // The event went to nobody; the file is still there to be taken.
    const bridge = backend(STATUS.path)
    bridge.handover = 'D:\\Elsewhere\\archive.dat'
    render(<App />)

    const file = await screen.findByRole('group', { name: 'Vault file' })
    await waitFor(() => expect(file).toHaveTextContent('archive.dat'))
    expect(bridge.handover).toBeNull()
  })

  it('leaves an open vault on the screen when a launch hands over its own file', async () => {
    // Whether the vault gives way for another file is the process's to decide,
    // and it would have locked it, and said so, first.
    const bridge = backend(STATUS.path, false)
    render(<App />)
    await typePassword('right')
    await screen.findByRole('heading', { name: 'notes.bak' })

    bridge.handover = STATUS.path
    await act(async () => emit(HANDED_OVER_EVENT))

    await waitFor(() => expect(bridge.handover).toBeNull())
    expect(screen.getByRole('heading', { name: 'notes.bak' })).toBeInTheDocument()
  })

  it('tells the process someone is there only while a vault is open', async () => {
    const bridge = backend(STATUS.path, false)
    const touches = () => bridge.calls.filter(([cmd]) => cmd === 'touch').length
    render(<App />)
    await screen.findByRole('heading', { name: 'Open a vault' })

    window.dispatchEvent(new Event('pointerdown'))
    expect(touches()).toBe(0)

    await typePassword('right')
    await screen.findByRole('heading', { name: 'notes.bak' })
    window.dispatchEvent(new Event('pointerdown'))
    expect(touches()).toBe(1)
  })

  it('says when the vault will lock', async () => {
    backend(STATUS.path, false)
    render(<App />)
    await typePassword('right')
    expect(await screen.findByText('locks after 15 min idle')).toBeInTheDocument()
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
    await userEvent.click(within(main).getByRole('button', { name: 'Settings' }))
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

  it('takes up what was written elsewhere while it is open', async () => {
    const state = backend(STATUS.path, false)
    render(<App />)
    await userEvent.type(await screen.findByLabelText('Master password'), 'right{Enter}')
    await waitFor(() => expect(screen.getByRole('button', { name: 'Lock' })).toBeInTheDocument())
    const rowsAsked = () => state.calls.filter(([cmd]) => cmd === 'items').length
    await waitFor(() => expect(rowsAsked()).toBeGreaterThan(0))
    const before = rowsAsked()

    await act(() => emit(CHANGED_EVENT))

    // The list is asked for again, and the status with it.
    await waitFor(() => expect(rowsAsked()).toBeGreaterThan(before))
    expect(state.calls.some(([cmd]) => cmd === 'status')).toBe(true)
    expect(screen.getByRole('button', { name: 'Lock' })).toBeInTheDocument()
  })

  it('says so when the file changed into something it cannot read, until it can', async () => {
    backend(STATUS.path, false)
    render(<App />)
    await userEvent.type(await screen.findByLabelText('Master password'), 'right{Enter}')
    await waitFor(() => expect(screen.getByRole('button', { name: 'Lock' })).toBeInTheDocument())
    expect(screen.queryByText(/cannot read the new version/)).not.toBeInTheDocument()

    await act(() => emit(UNREADABLE_EVENT))
    expect(await screen.findByText(/cannot read the new version/)).toBeInTheDocument()

    await act(() => emit(CHANGED_EVENT))
    await waitFor(() => expect(screen.queryByText(/cannot read the new version/)).not.toBeInTheDocument())
  })

  it('asks for a file again after a wipe, and says what happened to the last one', async () => {
    backend(STATUS.path, false)
    render(<App />)
    await userEvent.type(await screen.findByLabelText('Master password'), 'right{Enter}')
    const main = await screen.findByRole('main')
    await userEvent.click(await within(main).findByRole('button', { name: 'Settings' }))
    await userEvent.click(await screen.findByRole('button', { name: 'Wipe…' }))
    const dialog = await screen.findByRole('alertdialog')

    await userEvent.type(within(dialog).getByLabelText('Master password'), 'right')
    await userEvent.type(await within(dialog).findByLabelText(/Type the vault’s name/), 'notes.bak')
    await userEvent.click(within(dialog).getByRole('button', { name: 'Wipe it' }))

    const said = await screen.findByRole('status')
    expect(said).toHaveTextContent('The vault is wiped: 2 files overwritten and deleted.')
    expect(said).toHaveTextContent('The copy kept through github is not touched')
    expect(screen.getByText('No file chosen')).toBeInTheDocument()
    expect(screen.queryByText('notes.bak')).not.toBeInTheDocument()
  })
})
