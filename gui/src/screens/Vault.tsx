import { Lock } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { KeyValue, KeyValueRow } from '@/components/ui/key-value'
import { Panel, SectionLabel } from '@/components/ui/panel'
import type { VaultStatus } from '@/ipc'
import { ago, count, nowSeconds, size } from '@/lib/format'
import { text } from '@/text'

interface Props {
  status: VaultStatus
  onLock: () => void
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
 * The open vault, described: what it holds counted, where it is, how it is
 * sealed, when it last travelled.
 *
 * The same answer `sefy status` gives, from the same call into the core
 * (`Vault::status`), and under the same rule: it is about the vault and never
 * about what is in it. Not a title, not a tag name - the screen is asked
 * "is this the right file, is everything in it", often with somebody else in
 * the room. The items themselves are the next version's screen.
 */
export function Vault({ status, onLock }: Props) {
  const { sealing } = status
  const now = nowSeconds()

  return (
    <main className="flex min-h-0 flex-1 flex-col">
      <div className="flex shrink-0 items-center gap-3 border-b border-line px-4 py-2.5">
        <h1 className="min-w-0 truncate font-mono text-sm font-semibold text-text">{status.name}</h1>
        <span className="ml-auto flex items-center gap-2 text-xs text-dim">
          <span aria-hidden className="size-1.5 rounded-full bg-good" />
          {text.vault.unlocked}
        </span>
        <Button size="sm" onClick={onLock}>
          <Lock aria-hidden />
          {text.vault.lock}
        </Button>
      </div>

      <div className="grid min-h-0 flex-1 auto-rows-min grid-cols-1 gap-3 overflow-y-auto p-4 md:grid-cols-2">
        <Panel className="flex flex-col gap-3 p-4" aria-labelledby="contents-label">
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

        <Panel className="flex flex-col gap-3 p-4" aria-labelledby="file-label">
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

        <Panel className="flex flex-col gap-3 p-4" aria-labelledby="travel-label">
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

        <p className="self-center text-sm text-dim md:col-span-2">{text.vault.honest}</p>
      </div>

      <div className="flex shrink-0 flex-wrap items-center gap-x-3 border-t border-line px-4 py-1.5 font-mono text-2xs text-faint">
        <span>{count(status.items, text.vault.item, text.vault.items)}</span>
        <span aria-hidden>·</span>
        <span>
          {text.vault.format} v{sealing.format}
        </span>
        <span aria-hidden>·</span>
        <span>
          {text.vault.schema} {status.schema}
        </span>
      </div>
    </main>
  )
}
