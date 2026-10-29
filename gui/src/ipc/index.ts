import { invoke } from '@tauri-apps/api/core'
import type { Copied } from './generated/Copied'
import type { Failure } from './generated/Failure'
import type { Filter } from './generated/Filter'
import type { ItemCard } from './generated/ItemCard'
import type { ItemRow } from './generated/ItemRow'
import type { Opening } from './generated/Opening'
import type { Policy } from './generated/Policy'
import type { Revealed } from './generated/Revealed'
import type { Shortcut } from './generated/Shortcut'
import type { TagCount } from './generated/TagCount'
import type { VaultStatus } from './generated/VaultStatus'

/*
 * The commands the window may call, typed by the Rust that answers them
 * (`src-tauri/src/commands.rs`; the shapes are generated into `generated/`).
 * One function per command, so a page never spells a command's name or its
 * arguments itself.
 */

export type { Copied, Failure, Filter, ItemCard, ItemRow, Opening, Policy, Revealed, Shortcut, TagCount, VaultStatus }
export type { CardBody } from './generated/CardBody'
export type { CardField } from './generated/CardField'
export type { FailureKind } from './generated/FailureKind'

/** The file the window was started on. */
export const opening = () => invoke<Opening>('opening')

/** The system's file dialog, opening beside `near`; `null` when cancelled. */
export const chooseFile = (near: string | null) => invoke<string | null>('choose_file', { near })

/** Unlocks the vault at `path`. Rejects with a `Failure`. */
export const unlock = (path: string, password: string) =>
  invoke<VaultStatus>('unlock', { path, password })

/** Locks the open vault, takes back what is on the clipboard, and closes
 * quick search. */
export const lock = () => invoke<void>('lock')

/** What and where the open vault is. Rejects with a `Failure`. */
export const status = () => invoke<VaultStatus>('status')

/** The items `filter` lets through, best match first. */
export const items = (filter: Filter) => invoke<ItemRow[]>('items', { filter })

/** Every tag in use, with how many items carry it. */
export const tags = () => invoke<TagCount[]>('tags')

/** One item, as the card shows it: every secret named, none filled in. */
export const item = (id: number) => invoke<ItemCard>('item', { id })

/** One secret field, because someone asked to see it. */
export const reveal = (id: number, field: string) => invoke<Revealed>('reveal', { id, field })

/** Puts a field on the clipboard without it passing through the page; with
 * no field, what `sefy get` would take. */
export const copy = (id: number, field: string | null) => invoke<Copied>('copy', { id, field })

/** How long a copy stays on the clipboard, and the quick search shortcut. */
export const policy = () => invoke<Policy>('policy')

/** Puts the quick search window away. */
export const dismiss = () => invoke<void>('dismiss')

/** Whether a rejection is one of ours, rather than the bridge failing. */
export function isFailure(value: unknown): value is Failure {
  return (
    typeof value === 'object' &&
    value !== null &&
    'kind' in value &&
    'message' in value &&
    typeof (value as Failure).message === 'string'
  )
}
