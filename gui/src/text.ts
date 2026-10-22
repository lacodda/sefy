/**
 * Every word the window shows, in one place.
 *
 * English only, as the command line is. Kept together rather than written
 * where each is drawn, so that a second language, when it comes, is a second
 * copy of this object and not a hunt through every screen - and so that what
 * the window says about a vault can be read in one sitting and held to the
 * rule the splash and the status share: shape, never contents.
 */
export const text = {
  app: {
    name: 'sefy',
    tagline: 'An inconspicuous encrypted vault.',
  },
  window: {
    minimize: 'Minimize',
    maximize: 'Maximize',
    restore: 'Restore',
    close: 'Close',
  },
  splash: {
    unsealing: 'Unsealing the vault…',
    unsealingTip: 'The password is stretched on purpose: every guess at it costs the same second.',
  },
  unlock: {
    title: 'Open a vault',
    file: 'Vault file',
    noFile: 'No file chosen',
    dropHere: 'Drop the file to open it',
    browse: 'Browse…',
    password: 'Master password',
    show: 'Show password',
    hide: 'Hide password',
    submit: 'Unlock',
    foot: 'A vault file has no signature or extension — any name, anywhere.',
    refused: 'Wrong password, or this file is not a vault. There is no way to tell the two apart.',
    missing: 'There is no file at that path any more.',
    newerFormat: 'This vault was written by a newer sefy. Update sefy to open it.',
  },
  vault: {
    lock: 'Lock',
    unlocked: 'unlocked',
    contents: 'Contents',
    items: 'items',
    item: 'item',
    tags: 'tags',
    tag: 'tag',
    noItems: 'Nothing in it yet.',
    file: 'This vault',
    path: 'file',
    size: 'size',
    layout: 'layout',
    noSignature: 'no signature, no header',
    key: 'key',
    cipher: 'cipher',
    format: 'format',
    schema: 'schema',
    writtenByNewer: 'written by a newer sefy',
    plaintext: 'plaintext never touches the disk',
    travel: 'Copies and sync',
    synced: 'synced',
    never: 'never',
    through: 'through',
    copies: 'copies',
    copy: 'copy',
    noCopies: 'none yet; one is kept before a merge or an import changes the vault',
    besideTheVault: 'beside the vault',
    newest: 'newest',
    honest:
      'Inconspicuous, not deniable: the file draws no attention, but forensics can recognize a high-entropy blob. No protection against coercion is claimed.',
  },
} as const
