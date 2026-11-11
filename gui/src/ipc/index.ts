import { invoke } from '@tauri-apps/api/core'
import type { Copied } from './generated/Copied'
import type { Draft } from './generated/Draft'
import type { Edit } from './generated/Edit'
import type { Failure } from './generated/Failure'
import type { Filter } from './generated/Filter'
import type { Forms } from './generated/Forms'
import type { ItemCard } from './generated/ItemCard'
import type { ItemRow } from './generated/ItemRow'
import type { Opening } from './generated/Opening'
import type { PasswordChanged } from './generated/PasswordChanged'
import type { PasswordStrength } from './generated/PasswordStrength'
import type { Policy } from './generated/Policy'
import type { Recipe } from './generated/Recipe'
import type { Revealed } from './generated/Revealed'
import type { Shortcut } from './generated/Shortcut'
import type { TagCount } from './generated/TagCount'
import type { Transfer } from './generated/Transfer'
import type { Transport } from './generated/Transport'
import type { Transports } from './generated/Transports'
import type { VaultStatus } from './generated/VaultStatus'
import type { VersionRow } from './generated/VersionRow'
import type { WipeOutcome } from './generated/WipeOutcome'
import type { WipePreview } from './generated/WipePreview'
import type { Worth } from './generated/Worth'

/*
 * The commands the window may call, typed by the Rust that answers them
 * (`src-tauri/src/commands.rs`; the shapes are generated into `generated/`).
 * One function per command, so a page never spells a command's name or its
 * arguments itself.
 */

export type {
  Copied,
  Draft,
  Edit,
  Failure,
  Filter,
  Forms,
  ItemCard,
  ItemRow,
  Opening,
  PasswordChanged,
  PasswordStrength,
  Policy,
  Recipe,
  Revealed,
  Shortcut,
  TagCount,
  Transfer,
  Transport,
  Transports,
  VaultStatus,
  VersionRow,
  WipeOutcome,
  WipePreview,
  Worth,
}
export type { CardBody } from './generated/CardBody'
export type { CardField } from './generated/CardField'
export type { DraftContents } from './generated/DraftContents'
export type { DraftField } from './generated/DraftField'
export type { FailureKind } from './generated/FailureKind'
export type { FieldEdit } from './generated/FieldEdit'
export type { FieldForm } from './generated/FieldForm'
export type { KindForm } from './generated/KindForm'
export type { Value } from './generated/Value'

/** The file the window was started on. */
export const opening = () => invoke<Opening>('opening')

/** The file a second launch handed over, once. */
export const takeHandover = () => invoke<string | null>('take_handover')

/** Someone is using the window: the idle time starts over. */
export const touch = () => invoke<void>('touch')

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

/** What the editor can make, kind by kind, and how its generator starts. */
export const forms = () => invoke<Forms>('forms')

/** What a secret made by `recipe` would be worth. Rejects with why it cannot be
 * made; nothing is made either way. */
export const worth = (recipe: Recipe) => invoke<Worth>('worth', { recipe })

/** Adds what the editor drafted; resolves to the new item's id. A generated
 * secret is made in the process and never comes back here. */
export const addItem = (draft: Draft) => invoke<number>('add_item', { draft })

/** Changes the item `uuid` names; resolves to whether anything changed. */
export const editItem = (uuid: string, edit: Edit) => invoke<boolean>('edit_item', { uuid, edit })

/** Removes the item `uuid` names, with its history; resolves to whether it was
 * still there. */
export const deleteItem = (uuid: string) => invoke<boolean>('delete_item', { uuid })

/** Every version of an item, oldest first: every secret named, none filled in. */
export const history = (id: number) => invoke<VersionRow[]>('history', { id })

/** Brings back an earlier version of an item; says whether anything changed. */
export const restore = (id: number, version: string) => invoke<boolean>('restore', { id, version })

/** One secret field, because someone asked to see it; of an earlier version,
 * when one is named. */
export const reveal = (id: number, field: string, version?: string) =>
  invoke<Revealed>('reveal', version === undefined ? { id, field } : { id, field, version })

/** Puts a field on the clipboard without it passing through the page; with
 * no field, what `sefy get` would take. Of an earlier version, when one is
 * named. */
export const copy = (id: number, field: string | null, version?: string) =>
  invoke<Copied>('copy', version === undefined ? { id, field } : { id, field, version })

/** How long a copy stays on the clipboard, how long the vault stays unlocked
 * with nobody using it, what those can be set to, and the quick search
 * shortcut. */
export const policy = () => invoke<Policy>('policy')

/** Sets how long the vault stays unlocked and a copy stays on the clipboard. */
export const setPolicy = (lockAfterSeconds: number, clipboardSeconds: number) =>
  invoke<void>('set_policy', { lockAfterSeconds, clipboardSeconds })

/** How hard a new password would be to guess, worked out offline. */
export const strength = (password: string) => invoke<PasswordStrength>('strength', { password })

/** Changes the master password. Rejects with `refused` if `current` is not it. */
export const changePassword = (current: string, next: string) =>
  invoke<PasswordChanged>('change_password', { current, new: next })

/** What a wipe of the open vault would destroy. */
export const wipePreview = () => invoke<WipePreview>('wipe_preview')

/** Destroys the open vault, given its password and its name typed back. */
export const wipe = (password: string, name: string) => invoke<WipeOutcome>('wipe', { password, name })

/** The transports on this machine, usable or not. */
export const transports = () => invoke<Transports>('transports')

/** Pull, then push, through a transport. Rejects with `remote-refused` when the
 * copy there needs a password of its own. */
export const syncNow = (transport: string, remotePassword: string | null) =>
  invoke<Transfer>('sync_now', { transport, remotePassword })

/** Sends this vault up through a transport, replacing what is there. */
export const pushNow = (transport: string) => invoke<Transfer>('push_now', { transport })

/** What the last transfer did, since the vault was unlocked. */
export const lastTransfer = () => invoke<Transfer | null>('last_transfer')

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
