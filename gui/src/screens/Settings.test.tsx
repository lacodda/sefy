import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { mockIPC, mockWindows } from '@tauri-apps/api/mocks'
import { describe, expect, it, vi } from 'vitest'
import type { Failure, Policy, VaultStatus, WipeOutcome, WipePreview } from '@/ipc'
import { Settings } from '@/screens/Settings'

/*
 * The settings of the open vault, against a bridge that answers as the process
 * does. What is held to here: a limit is only ever one on offer, the password is
 * asked for before anything changes, and a wipe takes two things typed.
 */

const STATUS: VaultStatus = {
  name: 'notes.bak',
  path: 'C:\\Backups\\notes.bak',
  size: 86_118,
  items: 3,
  kinds: [{ kind: 'login', count: 3 }],
  tags: 2,
  schema: 5,
  writtenByNewer: false,
  lastSync: { at: 1_700_000_000, transport: 'relay', operation: 'sync' },
  copies: 1,
  newestCopy: 1_700_000_000,
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

const POLICY: Policy = {
  clipboardSeconds: 45,
  lockAfterSeconds: 900,
  clipboardChoices: [15, 30, 45, 90, 180],
  lockAfterChoices: [60, 300, 900, 1800, 3600, 14_400],
  quick: { keys: 'Ctrl+Alt+Space', active: true },
}

const PREVIEW: WipePreview = {
  name: 'notes.bak',
  directory: 'C:\\Backups',
  parts: [
    { name: 'notes.bak', role: 'the vault', size: 86_118 },
    { name: '.notes.bak.1', role: 'the most recent copy', size: 86_000 },
  ],
  remote: 'relay',
}

const REFUSED: Failure = { kind: 'refused', message: 'wrong password or not a vault file' }

function bridge() {
  const calls: [string, Record<string, unknown> | undefined][] = []
  mockWindows('main')
  mockIPC((cmd, args) => {
    const given = args as Record<string, unknown> | undefined
    calls.push([cmd, given])
    switch (cmd) {
      case 'strength':
        return given!.password === 'correct horse battery staple'
          ? { score: 4, warning: null }
          : { score: 0, warning: 'This is a very common password.' }
      case 'change_password':
        if (given!.current !== 'right') throw REFUSED
        return { resealed: 1, left: [], agentLocked: true }
      case 'wipe_preview':
        return PREVIEW
      case 'wipe':
        if (given!.password !== 'right') throw REFUSED
        return { destroyed: 2, vanished: 0, failed: [], remote: 'relay' } satisfies WipeOutcome
      default:
        return null
    }
  })
  return calls
}

const panel = (name: string) => screen.getByRole('group', { name })

describe('the Settings screen', () => {
  it('says what the vault is, in shape and never in contents', () => {
    bridge()
    render(<Settings status={STATUS} policy={POLICY} onWiped={() => undefined} />)

    expect(screen.getByText('3 items')).toBeInTheDocument()
    expect(screen.getByText(STATUS.path)).toBeInTheDocument()
    expect(screen.getByText(/salt 16 B \| nonce 24 B/)).toBeInTheDocument()
    expect(screen.getByText(/Inconspicuous, not deniable/)).toBeInTheDocument()
  })

  it('sets a limit to what is on offer, keeping the other', async () => {
    const calls = bridge()
    render(<Settings status={STATUS} policy={POLICY} onWiped={() => undefined} />)

    const trigger = within(panel('Security')).getByRole('combobox', { name: 'Auto-lock' })
    expect(trigger).toHaveTextContent('15 min')
    await userEvent.click(trigger)

    const options = await screen.findAllByRole('option')
    expect(options.map((option) => option.textContent)).toEqual([
      '1 min',
      '5 min',
      '15 min',
      '30 min',
      '1 hour',
      '4 hours',
    ])
    await userEvent.click(screen.getByRole('option', { name: '5 min' }))

    expect(calls.filter(([cmd]) => cmd === 'set_policy')).toEqual([
      ['set_policy', { lockAfterSeconds: 300, clipboardSeconds: 45 }],
    ])
  })

  it('shows the choice while it seals, and what the vault says once it has', async () => {
    let answer: () => void = () => undefined
    mockWindows('main')
    mockIPC((cmd) => (cmd === 'set_policy' ? new Promise<void>((resolve) => (answer = resolve)) : null))
    render(<Settings status={STATUS} policy={POLICY} onWiped={() => undefined} />)

    const trigger = screen.getByRole('combobox', { name: 'Auto-lock' })
    await userEvent.click(trigger)
    await userEvent.click(await screen.findByRole('option', { name: '5 min' }))
    expect(trigger).toHaveTextContent('5 min')

    // Nothing else said so: the vault still says fifteen, and that is what stands.
    answer()
    await waitFor(() => expect(trigger).toHaveTextContent('15 min'))
  })

  it('shows a limit a newer sefy set, as what it is', () => {
    bridge()
    render(
      <Settings status={STATUS} policy={{ ...POLICY, clipboardSeconds: 7 }} onWiped={() => undefined} />,
    )
    expect(screen.getByRole('combobox', { name: 'Clear the clipboard' })).toHaveTextContent('7 s')
  })

  it('takes the current password before changing it, and the new one twice', async () => {
    const calls = bridge()
    render(<Settings status={STATUS} policy={POLICY} onWiped={() => undefined} />)

    await userEvent.click(screen.getByRole('button', { name: 'Change…' }))
    const dialog = await screen.findByRole('dialog')
    const submit = within(dialog).getByRole('button', { name: 'Change it' })
    expect(submit).toBeDisabled()

    await userEvent.type(within(dialog).getByLabelText('Master password'), 'wrong')
    await userEvent.type(within(dialog).getByLabelText('New master password'), 'hunter2')
    expect(await within(dialog).findByText(/Guessable in moments: This is a very common password/)).toBeInTheDocument()

    await userEvent.type(within(dialog).getByLabelText('Repeat it'), 'different')
    expect(within(dialog).getByText('The two do not match.')).toBeInTheDocument()
    expect(submit).toBeDisabled()

    const repeat = within(dialog).getByLabelText('Repeat it')
    await userEvent.clear(repeat)
    await userEvent.type(repeat, 'hunter2')
    expect(submit).toBeEnabled()

    // A wrong current one stays where it is, saying so.
    await userEvent.click(submit)
    expect(await within(dialog).findByText('That is not the master password.')).toBeInTheDocument()

    const current = within(dialog).getByLabelText('Master password')
    await userEvent.clear(current)
    await userEvent.type(current, 'right')
    await userEvent.click(submit)

    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
    const status = await screen.findByRole('status')
    expect(status).toHaveTextContent('The master password is changed.')
    expect(status).toHaveTextContent('1 copy beside the vault sealed under it too.')
    expect(status).toHaveTextContent('The agent held the old one, and is locked.')
    expect(calls.filter(([cmd]) => cmd === 'change_password').at(-1)).toEqual([
      'change_password',
      { current: 'right', new: 'hunter2' },
    ])
  })

  it('wipes only with the name and the password typed, naming what goes', async () => {
    const calls = bridge()
    const onWiped = vi.fn()
    render(<Settings status={STATUS} policy={POLICY} onWiped={onWiped} />)

    await userEvent.click(screen.getByRole('button', { name: 'Wipe…' }))
    const dialog = await screen.findByRole('alertdialog')

    expect(await within(dialog).findByText('.notes.bak.1')).toBeInTheDocument()
    expect(within(dialog).getByText('the most recent copy')).toBeInTheDocument()
    expect(within(dialog).getByText(/kept through relay is not touched/)).toBeInTheDocument()
    expect(within(dialog).getByText(/An SSD, a snapshot/)).toBeInTheDocument()

    const submit = within(dialog).getByRole('button', { name: 'Wipe it' })
    expect(submit).toBeDisabled()

    await userEvent.type(within(dialog).getByLabelText('Master password'), 'wrong')
    expect(submit).toBeDisabled()

    const name = within(dialog).getByLabelText(/Type the vault’s name/)
    await userEvent.type(name, 'NOTES.BAK')
    expect(submit).toBeDisabled()
    await userEvent.clear(name)
    await userEvent.type(name, 'notes.bak')
    expect(submit).toBeEnabled()

    // Wrong password: nothing goes, and it says so.
    await userEvent.click(submit)
    expect(await within(dialog).findByRole('alert')).toHaveTextContent('Nothing was wiped')
    expect(onWiped).not.toHaveBeenCalled()

    const password = within(dialog).getByLabelText('Master password')
    await userEvent.clear(password)
    await userEvent.type(password, 'right')
    await userEvent.click(submit)

    await waitFor(() => expect(onWiped).toHaveBeenCalledOnce())
    expect(calls.filter(([cmd]) => cmd === 'wipe').at(-1)).toEqual([
      'wipe',
      { password: 'right', name: 'notes.bak' },
    ])
  })

  it('does not close a wipe on a stray click beside it', async () => {
    bridge()
    render(<Settings status={STATUS} policy={POLICY} onWiped={() => undefined} />)
    await userEvent.click(screen.getByRole('button', { name: 'Wipe…' }))
    await screen.findByRole('alertdialog')

    await userEvent.click(document.body)
    expect(screen.getByRole('alertdialog')).toBeInTheDocument()
  })
})
