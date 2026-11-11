import { useEffect, useState } from 'react'
import { History as HistoryIcon, Pencil, Trash2 } from 'lucide-react'
import { Alert } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
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
import * as ipc from '@/ipc'
import type { CardBody, CardField, Failure, ItemCard, VersionRow } from '@/ipc'
import { CopyButton, unexpected } from '@/lib/copies'
import { Button } from '@/components/ui/button'
import { ago, nowSeconds, size } from '@/lib/format'
import { text } from '@/text'

interface Props {
  card: ItemCard
  /** Starts with the history showing: how a conflict from the Sync screen
   * arrives here. */
  historyFirst?: boolean
  /** Goes up when what the vault holds may have moved on. */
  revision?: number
  /** Edit was pressed. */
  onEdit?: () => void
  /** The item is gone from the vault. */
  onDeleted?: () => void
}

/**
 * One item, opened: what `sefy show` prints, with Copy beside every field
 * and Show beside every secret.
 *
 * A secret is named and never filled in until someone asks to see it, and it
 * is hidden again by the same button, by opening another item, and by a
 * lock - the card is drawn per item, and the page forgets it when it goes.
 * Copy never brings the value into the page at all.
 */
export function Card({ card, historyFirst = false, revision = 0, onEdit, onDeleted }: Props) {
  const [failure, setFailure] = useState<Failure | null>(null)
  const [showingHistory, setShowingHistory] = useState(historyFirst)
  const [deleting, setDeleting] = useState(false)
  const now = nowSeconds()

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center gap-2">
        <Chip variant="accent" className="font-mono">
          {card.kind}
        </Chip>
        {card.tags.map((tag) => (
          <Chip key={tag}>{tag}</Chip>
        ))}
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
          {failure.message}
        </Alert>
      )}

      {showingHistory ? (
        <History id={card.id} revision={revision} onFailure={setFailure} />
      ) : (
        <Body id={card.id} body={card.body} onFailure={setFailure} />
      )}

      <div className="flex flex-wrap gap-2">
        <Button size="sm" onClick={onEdit}>
          <Pencil aria-hidden />
          {text.card.edit}
        </Button>
        {card.earlierVersions > 0 ? (
          <Button
            size="sm"
            aria-pressed={showingHistory}
            onClick={() => setShowingHistory((now) => !now)}
          >
            <HistoryIcon aria-hidden />
            {showingHistory ? text.card.current : text.card.historyOf(card.earlierVersions)}
          </Button>
        ) : null}
        <Button size="sm" variant="ghost" onClick={() => setDeleting(true)}>
          <Trash2 aria-hidden />
          {text.card.delete}
        </Button>
      </div>

      {deleting ? (
        <Delete
          card={card}
          onClose={() => setDeleting(false)}
          onDeleted={() => {
            setDeleting(false)
            onDeleted?.()
          }}
        />
      ) : null}

      <div className="flex flex-wrap gap-x-4 gap-y-1 border-t border-line pt-3 font-mono text-2xs text-faint">
        <span>
          {text.card.created} {ago(card.createdAt, now)}
        </span>
        <span>
          {text.card.modified} {ago(card.updatedAt, now)}
        </span>
        <span title={card.uuid}>
          {text.card.uuid} {card.uuid.slice(0, 8)}…{card.uuid.slice(-4)}
        </span>
      </div>
    </div>
  )
}

/**
 * The question before an item goes, saying what goes with it: its history, and
 * nothing that a copy elsewhere holds. Asked as `sefy rm` asks it.
 */
function Delete({ card, onClose, onDeleted }: { card: ItemCard; onClose: () => void; onDeleted: () => void }) {
  const [busy, setBusy] = useState(false)
  const [failure, setFailure] = useState<Failure | null>(null)

  const confirm = async () => {
    setBusy(true)
    setFailure(null)
    try {
      await ipc.deleteItem(card.uuid)
      onDeleted()
    } catch (cause) {
      setFailure(ipc.isFailure(cause) ? cause : unexpected(cause))
      setBusy(false)
    }
  }

  return (
    <ConfirmDialog open onOpenChange={(open) => !open && !busy && onClose()}>
      <ConfirmDialogPopup size="md">
        <ConfirmDialogHeader>
          <ConfirmDialogTitle>{text.card.deleteTitle(card.title)}</ConfirmDialogTitle>
          <ConfirmDialogDescription>{text.card.deleteBody(card.earlierVersions)}</ConfirmDialogDescription>
        </ConfirmDialogHeader>
        <ConfirmDialogBody className="flex flex-col gap-3">
          <p className="text-xs text-dim">{text.card.deleteElsewhere}</p>
          {failure === null ? null : (
            <Alert tone="bad" role="alert">
              {failure.message}
            </Alert>
          )}
        </ConfirmDialogBody>
        <ConfirmDialogActions>
          <ConfirmDialogClose render={<Button type="button" disabled={busy} />}>{text.card.cancel}</ConfirmDialogClose>
          <Button type="button" variant="danger" className="border border-bad" disabled={busy} onClick={() => void confirm()}>
            {busy ? text.card.deleting : text.card.deleteConfirm}
          </Button>
        </ConfirmDialogActions>
      </ConfirmDialogPopup>
    </ConfirmDialog>
  )
}

/**
 * What an item holds, or held: `version` names the earlier one, when it is not
 * the item as it is now. The same rules either way, and the same request for each
 * secret: nothing about a version being old makes a password in it less of one.
 */
function Body({
  id,
  body,
  version,
  onFailure,
}: {
  id: number
  body: CardBody
  version?: string
  onFailure: (failure: Failure) => void
}) {
  switch (body.shape) {
    case 'note':
      return (
        <section aria-label={text.card.text} className="flex flex-col gap-1.5">
          <div className="flex items-center justify-between gap-2">
            <Label>{text.card.text}</Label>
            <CopyButton id={id} field={null} version={version} label={text.card.copyOf('note')} onFailure={onFailure} />
          </div>
          <pre className="overflow-x-auto rounded-md border border-line bg-bg px-3.5 py-3 font-mono text-sm leading-relaxed whitespace-pre-wrap text-text">
            {body.text}
          </pre>
        </section>
      )
    case 'record':
      return (
        <div className="flex flex-col gap-3">
          {body.fields.map((field) => (
            <Field key={field.name} id={id} field={field} version={version} onFailure={onFailure} />
          ))}
        </div>
      )
    case 'file':
      return (
        <section aria-label={text.card.file} className="flex flex-col gap-1.5">
          <Label>{text.card.file}</Label>
          <Line>
            <span className="min-w-0 flex-1 truncate font-mono text-sm text-text">{body.filename}</span>
            <span className="font-mono text-xs text-dim">{size(body.size)}</span>
          </Line>
          <p className="text-xs text-dim">{text.card.fileHint}</p>
        </section>
      )
    case 'unknown':
      return <p className="text-sm text-dim">{text.card.unknown}</p>
  }
}

function Label({ children }: { children: string }) {
  return <span className="font-mono text-2xs tracking-wider text-faint uppercase">{children}</span>
}

function Line({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex min-h-10 items-center gap-2 rounded-md border border-line bg-bg px-3 py-1.5">{children}</div>
  )
}

/** One field of a record. */
function Field({
  id,
  field,
  version,
  onFailure,
}: {
  id: number
  field: CardField
  version?: string
  onFailure: (failure: Failure) => void
}) {
  return (
    <section aria-label={field.name} className="flex flex-col gap-1.5">
      <Label>{field.name}</Label>
      {field.shape === 'plain' ? (
        <Line>
          <span className="min-w-0 flex-1 truncate font-mono text-sm text-text" title={field.value}>
            {field.value}
          </span>
          {field.daysLeft === null ? null : <Due days={field.daysLeft} />}
          <CopyButton id={id} field={field.name} version={version} label={text.card.copyOf(field.name)} onFailure={onFailure} />
        </Line>
      ) : field.shape === 'secret' ? (
        <Secret id={id} name={field.name} version={version} onFailure={onFailure} />
      ) : (
        <Code id={id} name={field.name} period={field.period} version={version} onFailure={onFailure} />
      )}
    </section>
  )
}

/** How far off the date a record stops working is; amber once it is near or past. */
function Due({ days }: { days: number }) {
  const near = days <= 30
  return (
    <span className={near ? 'font-mono text-xs text-warn' : 'font-mono text-xs text-dim'}>
      {days < 0 ? text.card.expired(-days) : days === 0 ? text.card.expired(0) : text.card.expires(days)}
    </span>
  )
}

/** A secret field: dots until asked, the value until hidden again. */
function Secret({
  id,
  name,
  version,
  onFailure,
}: {
  id: number
  name: string
  version?: string
  onFailure: (failure: Failure) => void
}) {
  const [shown, setShown] = useState<string | null>(null)

  const toggle = () => {
    if (shown !== null) {
      setShown(null)
      return
    }
    ipc
      .reveal(id, name, version)
      .then((revealed) => setShown(revealed.value))
      .catch((cause: unknown) => onFailure(ipc.isFailure(cause) ? cause : unexpected(cause)))
  }

  return (
    <Line>
      <span
        className={
          shown === null
            ? 'min-w-0 flex-1 truncate font-mono text-sm tracking-widest text-dim'
            : 'min-w-0 flex-1 font-mono text-sm break-all text-text select-text'
        }
      >
        {shown ?? text.card.hidden}
      </span>
      <RevealButton shown={shown !== null} name={name} onClick={toggle} />
      <CopyButton id={id} field={name} version={version} label={text.card.copyOf(name)} onFailure={onFailure} />
    </Line>
  )
}

function RevealButton({ shown, name, onClick }: { shown: boolean; name: string; onClick: () => void }) {
  return (
    <Button
      variant="icon"
      size="sm"
      aria-pressed={shown}
      aria-label={`${shown ? text.card.hide : text.card.reveal} ${name}`}
      onClick={onClick}
      className="shrink-0 font-mono"
    >
      {shown ? text.card.hide : text.card.reveal}
    </Button>
  )
}

/** Seconds since the epoch, ticking once a second while mounted. */
function useClock(): number {
  const [now, setNow] = useState(nowSeconds)
  useEffect(() => {
    const timer = setInterval(() => setNow(nowSeconds()), 1000)
    return () => clearInterval(timer)
  }, [])
  return now
}

const RING = 2 * Math.PI * 10

/**
 * A one-time password: the ring of the code's life, the code when asked for,
 * and Copy - which copies the code, never the key it is made from.
 */
function Code({
  id,
  name,
  period,
  version,
  onFailure,
}: {
  id: number
  name: string
  period: number
  version?: string
  onFailure: (failure: Failure) => void
}) {
  const now = useClock()
  const left = period - (now % period)
  const step = Math.floor(now / period)
  const [shown, setShown] = useState(false)
  const [code, setCode] = useState<{ step: number; value: string } | null>(null)

  // A shown code is fetched again when its period turns over, so the screen
  // never shows a code that no longer works.
  useEffect(() => {
    if (!shown || code?.step === step) return
    let current = true
    ipc
      .reveal(id, name, version)
      .then((revealed) => current && setCode({ step, value: revealed.value }))
      .catch((cause: unknown) => current && onFailure(ipc.isFailure(cause) ? cause : unexpected(cause)))
    return () => {
      current = false
    }
  }, [shown, step, code?.step, id, name, version, onFailure])

  const value = shown && code !== null ? code.value : null
  const spaced = value === null ? text.card.codeHidden : value.replace(/^(\d{3})(\d{3})$/, '$1 $2')

  return (
    <Line>
      <svg viewBox="0 0 26 26" className="size-6 shrink-0 -rotate-90" aria-hidden>
        <circle cx="13" cy="13" r="10" fill="none" strokeWidth="3" className="stroke-line" />
        <circle
          cx="13"
          cy="13"
          r="10"
          fill="none"
          strokeWidth="3"
          strokeDasharray={RING}
          strokeDashoffset={RING * (1 - left / period)}
          className="stroke-accent transition-[stroke-dashoffset] duration-1000 ease-linear"
        />
      </svg>
      <span
        className={
          value === null
            ? 'min-w-0 flex-1 font-mono text-lg tracking-widest text-dim'
            : 'min-w-0 flex-1 font-mono text-lg font-semibold tracking-wider text-text tabular-nums'
        }
      >
        {spaced}
      </span>
      <span className="font-mono text-2xs text-faint tabular-nums">{text.card.newCode(left)}</span>
      <RevealButton
        shown={shown}
        name={text.card.code}
        onClick={() => {
          setShown((now) => !now)
          setCode(null)
        }}
      />
      <CopyButton id={id} field={name} version={version} label={text.card.copyOf(text.card.code)} onFailure={onFailure} />
    </Line>
  )
}

/**
 * Every version of an item, newest first, with who wrote it and which fields it
 * changed. Opening one shows what it held, by the same rules as the card, with the
 * way to bring it back; a version a conflict left behind opens by itself, since
 * that is what someone arriving from the Sync screen came to read.
 */
function History({
  id,
  revision,
  onFailure,
}: {
  id: number
  revision: number
  onFailure: (failure: Failure) => void
}) {
  const [versions, setVersions] = useState<VersionRow[] | null>(null)
  const [opened, setOpened] = useState<string | null>(null)
  const [restored, setRestored] = useState<number | null>(null)

  useEffect(() => {
    let live = true
    ipc
      .history(id)
      .then((found) => {
        if (!live) return
        setVersions(found)
        // Newest first from here on; a new version from a restore changes the
        // list, and an open one stays open.
        setOpened((now) => now ?? [...found].reverse().find((version) => version.conflict)?.uuid ?? null)
      })
      .catch((cause: unknown) => live && onFailure(ipc.isFailure(cause) ? cause : unexpected(cause)))
    return () => {
      live = false
    }
  }, [id, revision, onFailure])

  const restore = (version: VersionRow) => {
    ipc
      .restore(id, version.uuid)
      .then((changed) => {
        if (changed) setRestored(version.number)
      })
      .catch((cause: unknown) => onFailure(ipc.isFailure(cause) ? cause : unexpected(cause)))
  }

  const now = nowSeconds()

  return (
    <section aria-label={text.card.history} className="flex flex-col gap-2">
      {restored === null ? null : (
        <Alert tone="good" role="status">
          {text.card.restored(restored)}
        </Alert>
      )}
      {(versions ?? []).toReversed().map((version) => {
        const open = opened === version.uuid
        return (
          <div key={version.uuid} className="flex flex-col gap-3 rounded-md border border-line p-2">
            <Button
              variant="icon"
              size="sm"
              aria-expanded={open}
              onClick={() => setOpened(open ? null : version.uuid)}
              className="h-auto min-h-7 w-full flex-wrap justify-start gap-x-2 gap-y-1 py-1 text-left font-mono"
            >
              <span className="font-semibold text-text">v{version.number}</span>
              <span>{ago(version.madeAt, now)}</span>
              {version.device === null ? null : <span>· {version.device}</span>}
              {version.changed.length === 0 ? null : (
                <span className="min-w-0 truncate">· {version.changed.join(', ')}</span>
              )}
              <span className="ml-auto flex items-center gap-1.5">
                {version.conflict ? <Badge variant="warn">{text.card.keptFromConflict}</Badge> : null}
                {version.current ? <Badge variant="accent">{text.card.currentVersion}</Badge> : null}
              </span>
            </Button>
            {open ? (
              <>
                <Body
                  id={id}
                  body={version.body}
                  version={version.current ? undefined : version.uuid}
                  onFailure={onFailure}
                />
                {version.current ? null : (
                  <div>
                    <Button size="sm" onClick={() => restore(version)}>
                      {text.card.restore(version.number)}
                    </Button>
                  </div>
                )}
              </>
            ) : null}
          </div>
        )
      })}
    </section>
  )
}
