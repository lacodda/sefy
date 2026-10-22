import { invoke } from '@tauri-apps/api/core'
import type { Failure } from './generated/Failure'
import type { Opening } from './generated/Opening'
import type { VaultStatus } from './generated/VaultStatus'

/*
 * The commands the window may call, typed by the Rust that answers them
 * (`src-tauri/src/commands.rs`; the shapes are generated into `generated/`).
 * One function per command, so a page never spells a command's name or its
 * arguments itself.
 */

export type { Failure, Opening, VaultStatus }
export type { FailureKind } from './generated/FailureKind'

/** The file the window was started on. */
export const opening = () => invoke<Opening>('opening')

/** The system's file dialog, opening beside `near`; `null` when cancelled. */
export const chooseFile = (near: string | null) => invoke<string | null>('choose_file', { near })

/** Unlocks the vault at `path`. Rejects with a `Failure`. */
export const unlock = (path: string, password: string) =>
  invoke<VaultStatus>('unlock', { path, password })

/** Locks the open vault. */
export const lock = () => invoke<void>('lock')

/** What and where the open vault is. Rejects with a `Failure`. */
export const status = () => invoke<VaultStatus>('status')

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
