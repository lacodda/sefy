import { useEffect, useState } from 'react'
import { Alert } from '@/components/ui/alert'
import { Chip } from '@/components/ui/chip'
import * as ipc from '@/ipc'
import type { CardField, Failure, ItemCard } from '@/ipc'
import { CopyButton, unexpected } from '@/lib/copies'
import { Button } from '@/components/ui/button'
import { ago, nowSeconds, size } from '@/lib/format'
import { text } from '@/text'

interface Props {
  card: ItemCard
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
export function Card({ card }: Props) {
  const [failure, setFailure] = useState<Failure | null>(null)
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

      <Body card={card} onFailure={setFailure} />

      <div className="flex flex-wrap gap-x-4 gap-y-1 border-t border-line pt-3 font-mono text-2xs text-faint">
        <span>
          {text.card.created} {ago(card.createdAt, now)}
        </span>
        <span>
          {text.card.modified} {ago(card.updatedAt, now)}
        </span>
        {card.earlierVersions > 0 ? <span>{text.card.earlier(card.earlierVersions)}</span> : null}
        <span title={card.uuid}>
          {text.card.uuid} {card.uuid.slice(0, 8)}…{card.uuid.slice(-4)}
        </span>
      </div>
    </div>
  )
}

function Body({ card, onFailure }: { card: ItemCard; onFailure: (failure: Failure) => void }) {
  const { body } = card
  switch (body.shape) {
    case 'note':
      return (
        <section aria-label={text.card.text} className="flex flex-col gap-1.5">
          <div className="flex items-center justify-between gap-2">
            <Label>{text.card.text}</Label>
            <CopyButton id={card.id} field={null} label={text.card.copyOf('note')} onFailure={onFailure} />
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
            <Field key={field.name} id={card.id} field={field} onFailure={onFailure} />
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
function Field({ id, field, onFailure }: { id: number; field: CardField; onFailure: (failure: Failure) => void }) {
  return (
    <section aria-label={field.name} className="flex flex-col gap-1.5">
      <Label>{field.name}</Label>
      {field.shape === 'plain' ? (
        <Line>
          <span className="min-w-0 flex-1 truncate font-mono text-sm text-text" title={field.value}>
            {field.value}
          </span>
          {field.daysLeft === null ? null : <Due days={field.daysLeft} />}
          <CopyButton id={id} field={field.name} label={text.card.copyOf(field.name)} onFailure={onFailure} />
        </Line>
      ) : field.shape === 'secret' ? (
        <Secret id={id} name={field.name} onFailure={onFailure} />
      ) : (
        <Code id={id} name={field.name} period={field.period} onFailure={onFailure} />
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
function Secret({ id, name, onFailure }: { id: number; name: string; onFailure: (failure: Failure) => void }) {
  const [shown, setShown] = useState<string | null>(null)

  const toggle = () => {
    if (shown !== null) {
      setShown(null)
      return
    }
    ipc
      .reveal(id, name)
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
      <CopyButton id={id} field={name} label={text.card.copyOf(name)} onFailure={onFailure} />
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
  onFailure,
}: {
  id: number
  name: string
  period: number
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
      .reveal(id, name)
      .then((revealed) => current && setCode({ step, value: revealed.value }))
      .catch((cause: unknown) => current && onFailure(ipc.isFailure(cause) ? cause : unexpected(cause)))
    return () => {
      current = false
    }
  }, [shown, step, code?.step, id, name, onFailure])

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
      <CopyButton id={id} field={name} label={text.card.copyOf(text.card.code)} onFailure={onFailure} />
    </Line>
  )
}
