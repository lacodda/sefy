import { useEffect, useState, type FormEvent } from 'react'
import { ChevronDown } from 'lucide-react'
import { Alert } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import {
  ConfirmDialog,
  ConfirmDialogActions,
  ConfirmDialogBody,
  ConfirmDialogClose,
  ConfirmDialogDescription,
  ConfirmDialogHeader,
  ConfirmDialogPopup,
  ConfirmDialogTitle,
} from '@/components/ui/confirm-dialog'
import { Dialog, DialogActions, DialogBody, DialogDescription, DialogHeader, DialogPopup, DialogTitle } from '@/components/ui/dialog'
import { Field } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { KeyValue, KeyValueRow } from '@/components/ui/key-value'
import { Panel, SectionLabel } from '@/components/ui/panel'
import { PasswordField } from '@/components/ui/password-field'
import { Select, SelectIcon, SelectItem, SelectItemText, SelectPopup, SelectTrigger, SelectValue } from '@/components/ui/select'
import * as ipc from '@/ipc'
import type { Failure, PasswordChanged, PasswordStrength, Policy, VaultStatus, WipeOutcome, WipePreview } from '@/ipc'
import { unexpected } from '@/lib/copies'
import { ago, count, nowSeconds, size, span } from '@/lib/format'
import { text } from '@/text'

interface Props {
  status: VaultStatus
  policy: Policy | null
  /** The vault is gone: back to asking for a file, saying what happened. */
  onWiped: (outcome: WipeOutcome) => void
}

/** The file's layout, from the lengths the core states: what is at the
 * front, and that nothing in it says what it is. */
function layoutLine(sealing: VaultStatus['sealing']): string {
  return `salt ${sealing.saltLen} B | nonce ${sealing.nonceLen} B | ciphertext + ${sealing.tagLen} B tag`
}

/** The key derivation and its cost, in the words `sefy status` uses. */
function keyLine(sealing: VaultStatus['sealing']): string {
  return `${sealing.kdf} (${sealing.memoryKib / 1024} MiB, ${sealing.passes} passes, ${sealing.lanes} lanes)`
}

/**
 * What the open vault lets out and for how long, what it is, and how to end it.
 *
 * The figures here are the same answer `sefy status` gives, from the same call
 * into the core, and under the same rule: about the vault and never about what
 * is in it. Not a title, not a tag name - this screen is asked "is this the
 * right file, is everything in it", often with somebody else in the room.
 *
 * A setting is kept inside the vault, not beside it and not under the profile,
 * so there is no file saying sefy was here, and the command line obeys the same
 * numbers (`sefy_core::policy`).
 */
export function Settings({ status, policy, onWiped }: Props) {
  const { sealing } = status
  const now = nowSeconds()
  const [failure, setFailure] = useState<Failure | null>(null)
  const [changed, setChanged] = useState<PasswordChanged | null>(null)
  const [changing, setChanging] = useState(false)
  const [wiping, setWiping] = useState(false)
  // A setting takes a moment to seal, and the control shows the choice meanwhile
  // rather than snapping back to the old one. Only until the answer: what the
  // vault says is what is shown from then on.
  const [choosing, setChoosing] = useState<{ lockAfter: number; clipboard: number } | null>(null)

  const setLimits = (lockAfter: number, clipboard: number) => {
    setFailure(null)
    setChoosing({ lockAfter, clipboard })
    ipc
      .setPolicy(lockAfter, clipboard)
      .catch((cause: unknown) => setFailure(ipc.isFailure(cause) ? cause : unexpected(cause)))
      .finally(() => setChoosing(null))
  }
  const lockAfter = choosing?.lockAfter ?? policy?.lockAfterSeconds ?? 0
  const clipboard = choosing?.clipboard ?? policy?.clipboardSeconds ?? 0

  return (
    <div className="grid min-h-0 flex-1 auto-rows-min grid-cols-1 gap-2.5 overflow-y-auto lg:grid-cols-2">
      {failure === null ? null : (
        <Alert tone="bad" role="alert" className="lg:col-span-2">
          {failure.message}
        </Alert>
      )}

      <Panel role="group" className="flex flex-col gap-1 border border-line p-4" aria-labelledby="security-label">
        <SectionLabel id="security-label">{text.settings.security}</SectionLabel>
        <Row label={text.settings.lockAfter} hint={text.settings.lockAfterHint}>
          {policy === null ? null : (
            <Choice
              label={text.settings.lockAfter}
              value={lockAfter}
              choices={policy.lockAfterChoices}
              onChoose={(seconds) => setLimits(seconds, clipboard)}
            />
          )}
        </Row>
        <Row label={text.settings.clipboard} hint={text.settings.clipboardHint}>
          {policy === null ? null : (
            <Choice
              label={text.settings.clipboard}
              value={clipboard}
              choices={policy.clipboardChoices}
              onChoose={(seconds) => setLimits(lockAfter, seconds)}
            />
          )}
        </Row>
        <Row label={text.settings.password} hint={text.settings.passwordHint}>
          <Button
            size="sm"
            onClick={() => {
              setChanged(null)
              setChanging(true)
            }}
          >
            {text.settings.change}
          </Button>
        </Row>
        {changed === null ? null : (
          <Alert tone="good" role="status" className="mt-2">
            {text.settings.changedOutcome(changed)}
          </Alert>
        )}
      </Panel>

      <Panel role="group" className="flex flex-col gap-3 border border-line p-4" aria-labelledby="file-label">
        <SectionLabel id="file-label">{text.vault.file}</SectionLabel>
        <KeyValue className="font-mono text-xs">
          <KeyValueRow label={text.vault.path}>
            <span className="break-all">{status.path}</span>
          </KeyValueRow>
          {status.size === null ? null : (
            <KeyValueRow label={text.vault.size}>{size(status.size)}</KeyValueRow>
          )}
          <KeyValueRow label={text.vault.layout}>
            <span className="text-accent">{layoutLine(sealing)}</span>
            <span className="block text-dim">{text.vault.noSignature}</span>
          </KeyValueRow>
          <KeyValueRow label={text.vault.key}>{keyLine(sealing)}</KeyValueRow>
          <KeyValueRow label={text.vault.cipher}>{sealing.cipher}</KeyValueRow>
          <KeyValueRow label={text.vault.format}>
            v{sealing.format} · {text.vault.plaintext}
          </KeyValueRow>
          <KeyValueRow label={text.vault.schema}>
            {status.schema}
            {status.writtenByNewer ? ` (${text.vault.writtenByNewer})` : ''}
          </KeyValueRow>
        </KeyValue>
      </Panel>

      <Panel role="group" className="flex flex-col gap-3 border border-line p-4" aria-labelledby="contents-label">
        <SectionLabel id="contents-label">{text.vault.contents}</SectionLabel>
        <p className="font-mono text-2xl font-semibold text-text">
          {count(status.items, text.vault.item, text.vault.items)}
        </p>
        {status.kinds.length === 0 ? (
          <p className="text-sm text-dim">{text.vault.noItems}</p>
        ) : (
          <KeyValue>
            {status.kinds.map((kind) => (
              <KeyValueRow key={kind.kind} label={<span className="font-mono">{kind.kind}</span>}>
                <span className="font-mono">{kind.count}</span>
              </KeyValueRow>
            ))}
          </KeyValue>
        )}
        <p className="text-sm text-dim">{count(status.tags, text.vault.tag, text.vault.tags)}</p>
      </Panel>

      <Panel role="group" className="flex flex-col gap-3 border border-line p-4" aria-labelledby="travel-label">
        <SectionLabel id="travel-label">{text.vault.travel}</SectionLabel>
        <KeyValue className="text-sm">
          <KeyValueRow label={text.vault.synced}>
            {status.lastSync === null
              ? text.vault.never
              : `${ago(status.lastSync.at, now)} ${text.vault.through} ${status.lastSync.transport} (${status.lastSync.operation})`}
          </KeyValueRow>
          <KeyValueRow label={text.vault.copies}>
            {status.copies === 0
              ? text.vault.noCopies
              : `${count(status.copies, text.vault.copy, text.vault.copies)} ${text.vault.besideTheVault}` +
                (status.newestCopy === null ? '' : `, ${text.vault.newest} ${ago(status.newestCopy, now)}`)}
          </KeyValueRow>
        </KeyValue>
      </Panel>

      <p className="self-center text-sm text-dim lg:col-span-2">
        {text.vault.agent} {text.vault.honest}
      </p>

      <Panel role="group" className="flex items-center gap-4 border border-bad p-4 lg:col-span-2" aria-labelledby="danger-label">
        <div className="flex min-w-0 flex-1 flex-col gap-1">
          <SectionLabel id="danger-label" className="text-bad">
            {text.settings.danger}
          </SectionLabel>
          <p className="text-sm text-text">{text.settings.wipe}</p>
          <p className="text-xs text-dim">{text.settings.wipeHint}</p>
        </div>
        <Button variant="danger" className="shrink-0 border border-bad" onClick={() => setWiping(true)}>
          {text.settings.wipeAction}
        </Button>
      </Panel>

      {changing ? (
        <ChangePassword
          onClose={() => setChanging(false)}
          onChanged={(outcome) => {
            setChanged(outcome)
            setChanging(false)
          }}
        />
      ) : null}
      {wiping ? <Wipe onClose={() => setWiping(false)} onWiped={onWiped} /> : null}
    </div>
  )
}

/** One line of the security panel: what it is, a word about it, and the control. */
function Row({ label, hint, children }: { label: string; hint: string; children: React.ReactNode }) {
  return (
    <div className="flex items-center gap-3 border-b border-line py-2.5 last-of-type:border-b-0">
      <div className="flex min-w-0 flex-1 flex-col">
        <span className="text-sm text-text">{label}</span>
        <span className="text-xs text-dim">{hint}</span>
      </div>
      {children}
    </div>
  )
}

/** A time to choose. A value a newer sefy set stays on the list, as what it is. */
function Choice({
  label,
  value,
  choices,
  onChoose,
}: {
  label: string
  value: number
  choices: number[]
  onChoose: (seconds: number) => void
}) {
  const offered = choices.includes(value) ? choices : [...choices, value].sort((a, b) => a - b)
  return (
    <Select
      value={String(value)}
      items={offered.map((seconds) => ({ value: String(seconds), label: span(seconds) }))}
      onValueChange={(next) => {
        if (next !== null && Number(next) !== value) onChoose(Number(next))
      }}
    >
      <SelectTrigger size="sm" aria-label={label} className="w-24 shrink-0 font-mono">
        <SelectValue />
        <SelectIcon>
          <ChevronDown aria-hidden className="size-3.5 text-dim" />
        </SelectIcon>
      </SelectTrigger>
      <SelectPopup size="sm">
        {offered.map((seconds) => (
          <SelectItem key={seconds} value={String(seconds)} className="font-mono">
            <SelectItemText>{span(seconds)}</SelectItemText>
          </SelectItem>
        ))}
      </SelectPopup>
    </Select>
  )
}

/** The current master password and a new one, typed twice. */
function ChangePassword({
  onClose,
  onChanged,
}: {
  onClose: () => void
  onChanged: (outcome: PasswordChanged) => void
}) {
  const [current, setCurrent] = useState('')
  const [next, setNext] = useState('')
  const [repeat, setRepeat] = useState('')
  const [strength, setStrength] = useState<PasswordStrength | null>(null)
  const [busy, setBusy] = useState(false)
  const [wrong, setWrong] = useState(false)
  const [failure, setFailure] = useState<Failure | null>(null)

  useEffect(() => {
    if (next === '') return
    let live = true
    ipc
      .strength(next)
      .then((found) => live && setStrength(found))
      .catch(() => undefined)
    return () => {
      live = false
    }
  }, [next])

  const mismatch = repeat !== '' && repeat !== next
  const ready = current !== '' && next !== '' && repeat === next && !busy

  const submit = async (event: FormEvent) => {
    event.preventDefault()
    if (!ready) return
    setBusy(true)
    setWrong(false)
    setFailure(null)
    try {
      onChanged(await ipc.changePassword(current, next))
    } catch (cause) {
      const failed = ipc.isFailure(cause) ? cause : unexpected(cause)
      if (failed.kind === 'refused') setWrong(true)
      else setFailure(failed)
      setBusy(false)
    }
  }

  return (
    <Dialog open onOpenChange={(open) => !open && !busy && onClose()}>
      <DialogPopup size="sm">
        <form onSubmit={(event) => void submit(event)} className="flex min-h-0 flex-col">
          <DialogHeader>
            <DialogTitle>{text.settings.changeTitle}</DialogTitle>
            <DialogDescription>{text.settings.changeBody}</DialogDescription>
          </DialogHeader>
          <DialogBody className="flex flex-col gap-3">
            <Field label={text.settings.current} error={wrong ? text.settings.notCurrent : undefined}>
              <PasswordField
                autoComplete="off"
                showLabel={text.unlock.show}
                hideLabel={text.unlock.hide}
                value={current}
                onValueChange={setCurrent}
                className="font-mono"
              />
            </Field>
            <Field
              label={text.settings.next}
              help={
                strength === null || next === ''
                  ? undefined
                  : text.settings.strength(strength.score, strength.warning)
              }
            >
              <PasswordField
                autoComplete="off"
                showLabel={text.unlock.show}
                hideLabel={text.unlock.hide}
                value={next}
                onValueChange={setNext}
                className="font-mono"
              />
            </Field>
            <Field label={text.settings.repeat} error={mismatch ? text.settings.mismatch : undefined}>
              <PasswordField
                autoComplete="off"
                showLabel={text.unlock.show}
                hideLabel={text.unlock.hide}
                value={repeat}
                onValueChange={setRepeat}
                className="font-mono"
              />
            </Field>
            {failure === null ? null : (
              <Alert tone="bad" role="alert">
                {failure.message}
              </Alert>
            )}
          </DialogBody>
          <DialogActions>
            <Button type="button" disabled={busy} onClick={onClose}>
              {text.settings.cancel}
            </Button>
            <Button type="submit" variant="primary" disabled={!ready}>
              {busy ? text.settings.changing : text.settings.changeAction}
            </Button>
          </DialogActions>
        </form>
      </DialogPopup>
    </Dialog>
  )
}

/** Everything a wipe destroys, and two things typed before it does. */
function Wipe({ onClose, onWiped }: { onClose: () => void; onWiped: (outcome: WipeOutcome) => void }) {
  const [preview, setPreview] = useState<WipePreview | null>(null)
  const [password, setPassword] = useState('')
  const [name, setName] = useState('')
  const [busy, setBusy] = useState(false)
  const [failure, setFailure] = useState<Failure | null>(null)

  useEffect(() => {
    let live = true
    ipc
      .wipePreview()
      .then((found) => live && setPreview(found))
      .catch((cause: unknown) => live && setFailure(ipc.isFailure(cause) ? cause : unexpected(cause)))
    return () => {
      live = false
    }
  }, [])

  const ready = preview !== null && name.trim() === preview.name && password !== '' && !busy

  const submit = async (event: FormEvent) => {
    event.preventDefault()
    if (!ready) return
    setBusy(true)
    setFailure(null)
    try {
      onWiped(await ipc.wipe(password, name))
    } catch (cause) {
      const failed = ipc.isFailure(cause) ? cause : unexpected(cause)
      setFailure(failed.kind === 'refused' ? { ...failed, message: text.settings.wipeRefused } : failed)
      setBusy(false)
    }
  }

  return (
    <ConfirmDialog open onOpenChange={(open) => !open && !busy && onClose()}>
      <ConfirmDialogPopup size="lg">
        <form onSubmit={(event) => void submit(event)} className="flex min-h-0 flex-col">
          <ConfirmDialogHeader>
            <ConfirmDialogTitle>{text.settings.wipeTitle}</ConfirmDialogTitle>
            <ConfirmDialogDescription>{text.settings.wipeBody}</ConfirmDialogDescription>
          </ConfirmDialogHeader>
          <ConfirmDialogBody className="flex flex-col gap-3">
            {preview === null ? null : (
              <>
                <div className="flex flex-col gap-1">
                  <p className="font-mono text-2xs break-all text-faint">{preview.directory}</p>
                  <ul className="flex flex-col font-mono text-xs">
                    {preview.parts.map((part) => (
                      <li key={part.name} className="flex items-baseline gap-3 py-0.5">
                        <span className="min-w-0 flex-1 truncate text-text">{part.name}</span>
                        <span className="text-dim">{part.role}</span>
                        <span className="w-16 text-right text-faint">
                          {part.size === null ? '' : size(part.size)}
                        </span>
                      </li>
                    ))}
                  </ul>
                </div>
                {preview.remote === null ? null : (
                  <Alert tone="warn">{text.settings.wipeRemote(preview.remote)}</Alert>
                )}
                <p className="text-xs text-dim">{text.settings.wipeReach}</p>
                <Field label={text.settings.current}>
                  <PasswordField
                    autoComplete="off"
                    showLabel={text.unlock.show}
                    hideLabel={text.unlock.hide}
                    value={password}
                    onValueChange={setPassword}
                    className="font-mono"
                  />
                </Field>
                <Field
                  label={text.settings.typeName}
                  help={<span className="font-mono text-text select-text">{preview.name}</span>}
                >
                  <Input
                    autoComplete="off"
                    spellCheck={false}
                    value={name}
                    onChange={(event) => setName(event.target.value)}
                    className="font-mono"
                  />
                </Field>
              </>
            )}
            {failure === null ? null : (
              <Alert tone="bad" role="alert">
                {failure.message}
              </Alert>
            )}
          </ConfirmDialogBody>
          <ConfirmDialogActions>
            <ConfirmDialogClose render={<Button type="button" disabled={busy} />}>
              {text.settings.cancel}
            </ConfirmDialogClose>
            <Button type="submit" variant="danger" className="border border-bad" disabled={!ready}>
              {busy ? text.settings.wiping : text.settings.wipeConfirm}
            </Button>
          </ConfirmDialogActions>
        </form>
      </ConfirmDialogPopup>
    </ConfirmDialog>
  )
}
