import { useEffect, useState, type FormEvent } from 'react'
import { Alert } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Chip } from '@/components/ui/chip'
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
import { Dialog, DialogActions, DialogBody, DialogHeader, DialogPopup, DialogTitle, DialogDescription } from '@/components/ui/dialog'
import { Field } from '@/components/ui/field'
import { KeyValue, KeyValueRow } from '@/components/ui/key-value'
import { Panel, SectionLabel } from '@/components/ui/panel'
import { PasswordField } from '@/components/ui/password-field'
import { Spinner } from '@/components/ui/spinner'
import { StatusDot } from '@/components/ui/status-dot'
import * as ipc from '@/ipc'
import type { Failure, Transfer, Transport, Transports, VaultStatus } from '@/ipc'
import { unexpected } from '@/lib/copies'
import { ago, count, nowSeconds } from '@/lib/format'
import { text } from '@/text'

interface Props {
  status: VaultStatus
  /** Goes up when what the vault holds may have moved on. */
  revision?: number
  /** Opens an item with its history showing: where a conflict is settled. */
  onOpenHistory: (id: number) => void
}

/** What is being done through which transport, while it is. */
type Busy = { transport: string; what: 'sync' | 'push' } | null

/**
 * The transports on this machine, and what the last transfer through one did.
 *
 * The same `sync` and `push` as the command line's, through the same programs.
 * A transport is listed whether or not it works, with the reason when it does
 * not, because a broken one and a missing one call for opposite fixes. Setting
 * one up is the transport's own business, done in its environment before the
 * window starts: there is no configuration here for the window to keep, since
 * sefy keeps nothing on disk but the vault, its copies and its plugins.
 *
 * What came down is shown once, here, for as long as the vault stays unlocked,
 * with one thing singled out: an item changed on both machines. Nothing was
 * thrown away, and the row goes to the version that was kept (ADR-0013).
 */
export function Sync({ status, revision = 0, onOpenHistory }: Props) {
  const [transports, setTransports] = useState<Transports | null>(null)
  const [transfer, setTransfer] = useState<Transfer | null>(null)
  const [busy, setBusy] = useState<Busy>(null)
  const [failure, setFailure] = useState<Failure | null>(null)
  // The transport whose copy did not open with this vault's password.
  const [askFor, setAskFor] = useState<string | null>(null)
  const [pushTo, setPushTo] = useState<string | null>(null)

  useEffect(() => {
    let current = true
    ipc
      .transports()
      .then((found) => current && setTransports(found))
      .catch(() => current && setTransports({ found: [], directory: null, remote: '' }))
    return () => {
      current = false
    }
  }, [])

  // Asked again when the vault moves on: a sync that finished while this screen
  // was elsewhere has something to say when it comes back.
  useEffect(() => {
    let current = true
    ipc
      .lastTransfer()
      .then((last) => current && setTransfer(last))
      .catch(() => undefined)
    return () => {
      current = false
    }
  }, [revision])

  const run = async (next: Busy, ask: () => Promise<Transfer>) => {
    setBusy(next)
    setFailure(null)
    try {
      setTransfer(await ask())
      setAskFor(null)
    } catch (cause) {
      const failed = ipc.isFailure(cause) ? cause : unexpected(cause)
      if (failed.kind === 'remote-refused' && next !== null && askFor === null) {
        setAskFor(next.transport)
      } else {
        setFailure(failed)
      }
    } finally {
      setBusy(null)
    }
  }

  const sync = (transport: string, password: string | null) =>
    run({ transport, what: 'sync' }, () => ipc.syncNow(transport, password))

  const push = (transport: string) => run({ transport, what: 'push' }, () => ipc.pushNow(transport))

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-2.5 overflow-y-auto">
      <div className="flex flex-col gap-0.5 px-1">
        <h2 className="text-lg font-semibold text-text">{text.sync.title}</h2>
        <p className="text-sm text-dim">{text.sync.intro}</p>
      </div>

      {failure === null ? null : (
        <Alert
          tone="bad"
          role="alert"
          action={
            <Button variant="link" onClick={() => setFailure(null)}>
              {text.card.dismiss}
            </Button>
          }
        >
          <span className="whitespace-pre-wrap">{failure.message}</span>
        </Alert>
      )}

      <div className="grid auto-rows-min grid-cols-[repeat(auto-fill,minmax(260px,1fr))] gap-2.5">
        {transports === null ? (
          <Spinner label={text.sync.looking} />
        ) : (
          <>
            {transports.found.map((transport) => (
              <TransportCard
                key={transport.name}
                transport={transport}
                remote={transports.remote}
                last={status.lastSync?.transport === transport.name ? status.lastSync : null}
                busy={busy}
                onSync={() => void sync(transport.name, null)}
                onPush={() => setPushTo(transport.name)}
              />
            ))}
            <Panel className="flex flex-col justify-center gap-1 border border-dashed border-line bg-transparent p-4 text-sm text-dim">
              <span>{transports.found.length === 0 ? text.sync.noneYet : text.sync.addAnother}</span>
              {transports.directory === null ? null : (
                <span className="font-mono text-xs break-all text-faint select-text">{transports.directory}</span>
              )}
            </Panel>
          </>
        )}
      </div>

      <Outcome transfer={transfer} status={status} onOpenHistory={onOpenHistory} />

      {askFor === null ? null : (
        <RemotePassword
          transport={askFor}
          busy={busy !== null}
          failure={failure}
          onCancel={() => {
            setAskFor(null)
            setFailure(null)
          }}
          onSubmit={(password) => void sync(askFor, password)}
        />
      )}

      <ConfirmDialog open={pushTo !== null} onOpenChange={(open) => !open && setPushTo(null)}>
        <ConfirmDialogPopup>
          <ConfirmDialogHeader>
            <ConfirmDialogTitle>{text.sync.pushTitle}</ConfirmDialogTitle>
            <ConfirmDialogDescription>{text.sync.pushBody}</ConfirmDialogDescription>
          </ConfirmDialogHeader>
          <ConfirmDialogBody />
          <ConfirmDialogActions>
            <ConfirmDialogClose render={<Button />}>{text.settings.cancel}</ConfirmDialogClose>
            <Button
              variant="primary"
              onClick={() => {
                const transport = pushTo
                setPushTo(null)
                if (transport !== null) void push(transport)
              }}
            >
              {text.sync.push}
            </Button>
          </ConfirmDialogActions>
        </ConfirmDialogPopup>
      </ConfirmDialog>
    </div>
  )
}

function TransportCard({
  transport,
  remote,
  last,
  busy,
  onSync,
  onPush,
}: {
  transport: Transport
  remote: string
  last: VaultStatus['lastSync']
  busy: Busy
  onSync: () => void
  onPush: () => void
}) {
  const usable = transport.problem === null
  const canPull = transport.operations.includes('pull')
  const canPush = transport.operations.includes('push')
  const mine = busy?.transport === transport.name

  return (
    <Panel role="group" className="flex flex-col gap-3 border border-line p-4" aria-label={transport.name}>
      <div className="flex items-center gap-2">
        <h3 className="min-w-0 truncate font-mono text-sm font-semibold text-text">{transport.name}</h3>
        {transport.version === null ? null : (
          <Chip className="font-mono">{transport.version}</Chip>
        )}
      </div>

      <StatusDot
        status={!usable ? 'bad' : last === null ? 'neutral' : 'good'}
        showLabel
        label={!usable ? text.sync.unusable : last === null ? text.sync.neverHere : text.sync.reached}
        className="text-sm text-dim"
      />
      {usable ? null : <p className="text-xs break-words text-bad">{transport.problem}</p>}

      <KeyValue className="font-mono text-xs">
        <KeyValueRow label={text.sync.remote}>{remote}</KeyValueRow>
        <KeyValueRow label={text.sync.lastSync}>
          {last === null ? text.vault.never : `${ago(last.at, nowSeconds())} (${last.operation})`}
        </KeyValueRow>
        <KeyValueRow label={text.sync.protocol}>{transport.protocol === null ? '?' : `v${transport.protocol}`}</KeyValueRow>
      </KeyValue>

      <div className="mt-auto flex flex-wrap items-center gap-2">
        <Button
          variant="primary"
          size="sm"
          disabled={!usable || !canPull || !canPush || busy !== null}
          onClick={onSync}
        >
          {mine && busy?.what === 'sync' ? text.sync.syncing : text.sync.now}
        </Button>
        <Button size="sm" disabled={!usable || !canPush || busy !== null} onClick={onPush}>
          {mine && busy?.what === 'push' ? text.sync.pushing : text.sync.pushAction}
        </Button>
      </div>
    </Panel>
  )
}

/** What the last transfer did, and the one part of it someone has to look at. */
function Outcome({
  transfer,
  status,
  onOpenHistory,
}: {
  transfer: Transfer | null
  status: VaultStatus
  onOpenHistory: (id: number) => void
}) {
  if (transfer === null) {
    return (
      <Panel role="group" className="flex flex-col gap-2 border border-line p-4" aria-labelledby="last-sync-label">
        <SectionLabel id="last-sync-label">{text.sync.lastTitle}</SectionLabel>
        <p className="text-sm text-dim">
          {status.lastSync === null
            ? text.sync.neverAnywhere
            : text.sync.lastElsewhere(
                ago(status.lastSync.at, nowSeconds()),
                status.lastSync.transport,
                status.lastSync.operation,
              )}
        </p>
      </Panel>
    )
  }

  const nothing =
    transfer.added.length === 0 && transfer.updated.length === 0 && transfer.conflicts.length === 0

  return (
    <Panel role="group" className="flex flex-col gap-2 border border-line p-4" aria-labelledby="last-sync-label">
      <SectionLabel id="last-sync-label">
        {text.sync.lastOf(transfer.operation, transfer.transport, ago(transfer.at, nowSeconds()))}
      </SectionLabel>

      {transfer.sendFailure === null ? null : (
        <Alert tone="warn" role="alert">
          <span className="whitespace-pre-wrap">{text.sync.notSent(transfer.sendFailure)}</span>
        </Alert>
      )}

      {transfer.operation === 'push' ? (
        <p className="text-sm text-dim">{text.sync.pushed}</p>
      ) : nothing ? (
        <p className="text-sm text-dim">{text.sync.agreed}</p>
      ) : (
        <ul className="flex flex-col font-mono text-sm">
          {transfer.conflicts.map((conflict) => (
            <li key={`conflict-${conflict.id}`} className="flex items-center gap-3 rounded-sm px-2 py-1 odd:bg-bg">
              <Button
                variant="link"
                className="min-w-0 flex-1 justify-start truncate text-warn"
                onClick={() => onOpenHistory(conflict.id)}
              >
                {conflict.title}
              </Button>
              <Badge variant="warn">{text.sync.keptBoth}</Badge>
            </li>
          ))}
          {transfer.updated.map((row) => (
            <li key={`updated-${row.id}`} className="flex items-center gap-3 rounded-sm px-2 py-1 odd:bg-bg">
              <span className="min-w-0 flex-1 truncate">{row.title}</span>
              <Badge variant="accent">{text.sync.updated}</Badge>
            </li>
          ))}
          {transfer.added.map((row) => (
            <li key={`added-${row.id}`} className="flex items-center gap-3 rounded-sm px-2 py-1 odd:bg-bg">
              <span className="min-w-0 flex-1 truncate">{row.title}</span>
              <Badge variant="good">{text.sync.added}</Badge>
            </li>
          ))}
        </ul>
      )}

      {transfer.conflicts.length === 0 ? null : <p className="text-xs text-dim">{text.sync.keptBothNote}</p>}

      <p className="font-mono text-2xs text-faint">
        {[
          transfer.operation === 'sync' ? `${transfer.unchanged} ${text.sync.unchanged}` : null,
          transfer.versions > 0 ? count(transfer.versions, text.sync.version, text.sync.versions) : null,
          transfer.unsupported > 0 ? text.sync.unsupported(transfer.unsupported) : null,
          ...transfer.said,
        ]
          .filter((part) => part !== null)
          .join(' · ')}
      </p>
      {transfer.keptCopy === null ? null : (
        <p className="font-mono text-2xs break-all text-faint">{text.sync.kept(transfer.keptCopy)}</p>
      )}
    </Panel>
  )
}

/** The password of the copy at the remote, when it is not this vault's. */
function RemotePassword({
  transport,
  busy,
  failure,
  onCancel,
  onSubmit,
}: {
  transport: string
  busy: boolean
  failure: Failure | null
  onCancel: () => void
  onSubmit: (password: string) => void
}) {
  const [password, setPassword] = useState('')

  const submit = (event: FormEvent) => {
    event.preventDefault()
    if (password !== '' && !busy) onSubmit(password)
  }

  return (
    <Dialog open onOpenChange={(open) => !open && onCancel()}>
      <DialogPopup size="sm">
        <form onSubmit={submit} className="flex min-h-0 flex-col">
          <DialogHeader>
            <DialogTitle>{text.sync.remotePasswordTitle}</DialogTitle>
            <DialogDescription>{text.sync.remotePasswordBody(transport)}</DialogDescription>
          </DialogHeader>
          <DialogBody className="flex flex-col gap-3">
            <Field label={text.sync.remotePassword}>
              <PasswordField
                autoComplete="off"
                showLabel={text.unlock.show}
                hideLabel={text.unlock.hide}
                value={password}
                onValueChange={setPassword}
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
            <Button type="button" onClick={onCancel}>
              {text.settings.cancel}
            </Button>
            <Button type="submit" variant="primary" disabled={password === '' || busy}>
              {busy ? text.sync.syncing : text.sync.now}
            </Button>
          </DialogActions>
        </form>
      </DialogPopup>
    </Dialog>
  )
}
