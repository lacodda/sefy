import { useEffect, useRef, useState, type KeyboardEvent } from 'react'
import { Lock } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Chip } from '@/components/ui/chip'
import { EmptyState } from '@/components/ui/empty-state'
import { ListDetail, ListDetailBack, Pane, useListDetailTurn } from '@/components/ui/frame'
import { RowButton } from '@/components/ui/list-row'
import { SearchField } from '@/components/ui/search-field'
import { SectionNav } from '@/components/ui/section-nav'
import * as ipc from '@/ipc'
import type { Filter, ItemCard, ItemRow, Policy, TagCount, VaultStatus } from '@/ipc'
import { CopiesProvider } from '@/lib/copies'
import { ago, count, nowSeconds } from '@/lib/format'
import { About } from '@/screens/About'
import { Card } from '@/screens/Card'
import { text } from '@/text'

interface Props {
  status: VaultStatus
  onLock: () => void
}

/** What the rail has picked: every item, one kind, or the vault itself. */
type View = { at: 'items'; kind: string | null } | { at: 'about' }

/** The id the rail gives "every item"; kinds go by their stored names. */
const ALL = '*'
const ABOUT = 'about'

/** Headings the generic rule below would get wrong. */
const HEADINGS: Record<string, string> = {
  wifi: 'Wi-Fi networks',
  bank: 'Bank accounts',
}

/** The kind's name as a heading: `login` is `Logins`, `api-token` is `API
 * tokens` - a short first word is an abbreviation and is written as one. A
 * kind this build does not know is named the same way, from what it is
 * stored as. */
function kindHeading(kind: string): string {
  const known = HEADINGS[kind]
  if (known !== undefined) return known
  const [first = '', ...rest] = kind.split('-')
  const head = first.length <= 3 ? first.toUpperCase() : first.charAt(0).toUpperCase() + first.slice(1)
  return [head, ...rest].join(' ') + 's'
}

/**
 * The open vault: the rail, the list, and the item that is open.
 *
 * The list is the core's search (`sefy find`, through `items`): text in the
 * box, narrowed by a kind or a tag from the rail, with titles that match
 * ahead of matches inside. The card is `sefy show`: every field named, every
 * secret hidden until asked for, and Copy beside each.
 *
 * Nothing here holds a secret for longer than it is on screen: the card is
 * drawn per item, and a lock unmounts all of it.
 */
export function Vault({ status, onLock }: Props) {
  const [view, setView] = useState<View>({ at: 'items', kind: null })
  const [query, setQuery] = useState('')
  const [tag, setTag] = useState<string | null>(null)
  const [rows, setRows] = useState<ItemRow[] | null>(null)
  const [tags, setTags] = useState<TagCount[]>([])
  const [selected, setSelected] = useState<number | null>(null)
  const [card, setCard] = useState<ItemCard | null>(null)
  const [policy, setPolicy] = useState<Policy | null>(null)
  const searchRef = useRef<HTMLInputElement>(null)
  const listRef = useRef<HTMLDivElement>(null)
  const titleRef = useRef<HTMLHeadingElement>(null)

  const kind = view.at === 'items' ? view.kind : null
  const typed = query.trim()

  useEffect(() => {
    let current = true
    ipc
      .tags()
      .then((found) => current && setTags(found))
      .catch(() => undefined)
    ipc
      .policy()
      .then((found) => current && setPolicy(found))
      .catch(() => undefined)
    return () => {
      current = false
    }
  }, [])

  // The list follows the box and the rail. An answer that comes back after a
  // newer question was asked is dropped rather than drawn over it.
  useEffect(() => {
    let current = true
    const filter: Filter = { text: typed === '' ? null : typed, kind, tag }
    ipc
      .items(filter)
      .then((found) => current && setRows(found))
      .catch(() => current && setRows([]))
    return () => {
      current = false
    }
  }, [typed, kind, tag])

  // The open item is one the list still shows: narrowing the list closes
  // what it no longer holds.
  const open = selected !== null && rows?.some((row) => row.id === selected) ? selected : null

  useEffect(() => {
    if (open === null) return
    let current = true
    ipc
      .item(open)
      .then((found) => current && setCard(found))
      .catch(() => current && setCard(null))
    return () => {
      current = false
    }
  }, [open])

  // The card on screen is the open item's, and nothing once none is: a card
  // fetched for an item since closed is not drawn.
  const shown = card !== null && card.id === open ? card : null

  const turn = useListDetailTurn({
    selected: open === null ? null : String(open),
    title: titleRef,
    rowOf: (key) => listRef.current?.querySelector<HTMLElement>(`[data-row-key="${CSS.escape(key)}"]`) ?? null,
    fallback: searchRef,
  })

  const choose = (id: number | null) => {
    turn(id === null ? null : String(id))
    setSelected(id)
  }

  /* Up and Down walk the list and open what they land on, as a mail client
   * does; Home and End go to the ends. Down from the search box steps into
   * the list, and Enter there opens the first thing found. */
  const walk = (event: KeyboardEvent<HTMLDivElement>) => {
    const found = [...(listRef.current?.querySelectorAll<HTMLElement>('[data-row-key]') ?? [])]
    if (found.length === 0) return
    const at = found.findIndex((element) => element === document.activeElement)
    const next =
      event.key === 'ArrowDown'
        ? Math.min(at + 1, found.length - 1)
        : event.key === 'ArrowUp'
          ? Math.max(at - 1, 0)
          : event.key === 'Home'
            ? 0
            : event.key === 'End'
              ? found.length - 1
              : null
    if (next === null || at === -1) return
    event.preventDefault()
    found[next]!.focus()
    const key = Number(found[next]!.dataset.rowKey)
    if (key !== open) choose(key)
  }

  const fromSearch = (event: KeyboardEvent<HTMLInputElement>) => {
    const first = listRef.current?.querySelector<HTMLElement>('[data-row-key]')
    if (first === null || first === undefined) return
    if (event.key === 'ArrowDown') {
      event.preventDefault()
      first.focus()
    } else if (event.key === 'Enter') {
      event.preventDefault()
      first.focus()
      choose(Number(first.dataset.rowKey))
    }
  }

  const railItems = [
    { id: ALL, label: <Counted label={text.browse.all} count={status.items} /> },
    ...status.kinds.map((entry) => ({
      id: entry.kind,
      label: <Counted label={kindHeading(entry.kind)} count={entry.count} />,
    })),
  ]

  const now = nowSeconds()

  return (
    <CopiesProvider>
      <main className="flex min-h-0 flex-1 flex-col">
        <div className="flex shrink-0 items-center gap-3 border-b border-line px-4 py-2">
          <h1 className="min-w-0 max-w-[30%] truncate font-mono text-sm font-semibold text-text" title={status.path}>
            {status.name}
          </h1>
          <SearchField
            ref={searchRef}
            value={query}
            onValueChange={(value) => {
              setQuery(value)
              if (view.at === 'about') setView({ at: 'items', kind: null })
            }}
            onKeyDown={fromSearch}
            placeholder={text.browse.search}
            aria-label={text.browse.search}
            clearLabel={text.browse.clear}
            shortcut={{ id: 'search', label: text.browse.search, keys: 'Mod+K' }}
            className="w-full max-w-md"
          />
          <span className="ml-auto flex shrink-0 items-center gap-2 text-xs text-dim">
            <span aria-hidden className="size-1.5 rounded-full bg-good" />
            {text.vault.unlocked}
          </span>
          <Button size="sm" onClick={onLock}>
            <Lock aria-hidden />
            {text.vault.lock}
          </Button>
        </div>

        <div className="flex min-h-0 flex-1 gap-2.5 p-2.5">
          <nav aria-label={text.browse.items} className="flex w-rail shrink-0 flex-col gap-4 overflow-y-auto">
            <SectionNav
              label={text.browse.items}
              items={railItems}
              activeId={view.at === 'items' ? (view.kind ?? ALL) : undefined}
              onSelect={(id) => setView({ at: 'items', kind: id === ALL ? null : id })}
            />
            {tags.length === 0 ? null : (
              <div className="flex flex-col gap-2 px-2.5">
                <h2 className="caption">{text.browse.tags}</h2>
                <div className="flex flex-wrap gap-1.5">
                  {tags.map((entry) => (
                    <Chip
                      key={entry.tag}
                      variant={tag === entry.tag ? 'accent' : 'outline'}
                      pressed={tag === entry.tag}
                      onPressedChange={(pressed) => setTag(pressed ? entry.tag : null)}
                      count={entry.count}
                    >
                      {entry.tag}
                    </Chip>
                  ))}
                </div>
              </div>
            )}
            <SectionNav
              label={text.browse.about}
              labelHidden
              items={[{ id: ABOUT, label: text.browse.about }]}
              activeId={view.at === 'about' ? ABOUT : undefined}
              onSelect={() => setView({ at: 'about' })}
              className="mt-auto"
            />
          </nav>

          {view.at === 'about' ? (
            <About status={status} />
          ) : rows !== null && rows.length === 0 && typed === '' && kind === null && tag === null ? (
            <EmptyState title={text.browse.nothingYet} body={text.browse.nothingYetBody} className="flex-1" />
          ) : (
            <ListDetail
              open={shown !== null}
              list={
                <Pane label={text.browse.list} bodyClassName="p-1">
                  {rows !== null && rows.length === 0 ? (
                    <EmptyState
                      plain
                      variant="filtered"
                      title={text.browse.nothingFound}
                      body={text.browse.nothingFoundBody}
                      className="p-2"
                    />
                  ) : (
                    <div ref={listRef} onKeyDown={walk} className="flex flex-col">
                      {(rows ?? []).map((row) => (
                        <RowButton
                          key={row.id}
                          data-row-key={row.id}
                          selected={row.id === open}
                          start={<KindBadge kind={row.kind} />}
                          description={
                            row.known
                              ? [row.tags.join(', '), `${text.browse.edited} ${ago(row.updatedAt, now)}`]
                                  .filter((part) => part !== '')
                                  .join(' · ')
                              : text.browse.needsNewer
                          }
                          onClick={() => choose(row.id)}
                        >
                          <span className="font-mono">{row.title}</span>
                        </RowButton>
                      ))}
                    </div>
                  )}
                </Pane>
              }
              detail={
                shown === null ? (
                  <EmptyState title={text.browse.nothingOpen} body={text.browse.nothingOpenBody} className="flex-1" />
                ) : (
                  <Pane
                    label={shown.title}
                    head={
                      <>
                        <ListDetailBack label={text.browse.back} onClick={() => choose(null)} />
                        <h2
                          ref={titleRef}
                          tabIndex={-1}
                          className="min-w-0 flex-1 truncate font-mono text-lg font-semibold text-text outline-none"
                        >
                          {shown.title}
                        </h2>
                      </>
                    }
                    bodyClassName="p-4"
                  >
                    <Card key={shown.id} card={shown} />
                  </Pane>
                )
              }
            />
          )}
        </div>

        <div className="flex shrink-0 flex-wrap items-center gap-x-3 border-t border-line px-4 py-1.5 font-mono text-2xs text-faint">
          <span>{count(status.items, text.vault.item, text.vault.items)}</span>
          <span aria-hidden>·</span>
          <span>
            {text.vault.format} v{status.sealing.format}
          </span>
          {policy === null ? null : (
            <>
              <span aria-hidden>·</span>
              <span>{text.status.clears(policy.clipboardSeconds)}</span>
              <span aria-hidden>·</span>
              <span className={policy.quick.active ? undefined : 'text-warn'}>
                {policy.quick.active ? text.status.quick(policy.quick.keys) : text.status.quickTaken(policy.quick.keys)}
              </span>
            </>
          )}
        </div>
      </main>
    </CopiesProvider>
  )
}

/** A rail entry's name and how many it holds. */
function Counted({ label, count }: { label: string; count: number }) {
  return (
    <span className="flex items-center gap-2">
      <span className="min-w-0 flex-1 truncate">{label}</span>
      <span className="font-mono text-2xs text-faint tabular-nums">{count}</span>
    </span>
  )
}

/** The short tag at the start of a row, where the kind's own name is long. */
const SHORT: Record<string, string> = { login: 'log', 'api-token': 'api' }

/** The kind, as the short tag at the start of a row: LOG, NOTE, CARD. */
function KindBadge({ kind }: { kind: string }) {
  const short = SHORT[kind] ?? (kind.split('-')[0] ?? kind).slice(0, 4)
  return (
    <span className="w-10 shrink-0 rounded-sm border border-line py-0.5 text-center font-mono text-2xs font-semibold tracking-wide text-dim uppercase">
      {short}
    </span>
  )
}
