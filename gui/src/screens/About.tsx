import { KeyValue, KeyValueRow } from '@/components/ui/key-value'
import { Panel, SectionLabel } from '@/components/ui/panel'
import type { VaultStatus } from '@/ipc'
import { ago, count, nowSeconds, size } from '@/lib/format'
import { text } from '@/text'

interface Props {
  status: VaultStatus
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
 * about what is in it. Not a title, not a tag name - the view is asked "is
 * this the right file, is everything in it", often with somebody else in the
 * room.
 */
export function About({ status }: Props) {
  const { sealing } = status
  const now = nowSeconds()

  return (
    <div className="grid min-h-0 flex-1 auto-rows-min grid-cols-1 gap-2.5 overflow-y-auto lg:grid-cols-2">
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

      <p className="self-center text-sm text-dim lg:col-span-2">{text.vault.honest}</p>
    </div>
  )
}
