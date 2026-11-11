import { useEffect, useId, useState, type FormEvent, type ReactNode } from 'react'
import { ChevronDown, Plus, X } from 'lucide-react'
import { Alert } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Field } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { NumberField } from '@/components/ui/number-field'
import { PasswordField } from '@/components/ui/password-field'
import { Segment, SegmentedControl } from '@/components/ui/segmented-control'
import { Select, SelectIcon, SelectItem, SelectItemText, SelectPopup, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Switch } from '@/components/ui/switch'
import { TagInput } from '@/components/ui/tag-input'
import { Textarea } from '@/components/ui/textarea'
import * as ipc from '@/ipc'
import type { Draft, DraftField, Edit, Failure, FieldEdit, FieldForm, Forms, ItemCard, KindForm, Recipe, Value, Worth } from '@/ipc'
import { unexpected } from '@/lib/copies'
import { text } from '@/text'

/** What the editor is open on: a kind to make an item of, or an item to change. */
export type Editing = { at: 'new'; kind: string } | { at: 'edit'; card: ItemCard }

interface Props {
  forms: Forms
  editing: Editing
  /** Saved: the id of the item made or changed, and whether anything changed. */
  onSaved: (id: number, changed: boolean) => void
  onCancel: () => void
  /** Whether there is something here that leaving would lose. */
  onDirtyChange?: (dirty: boolean) => void
}

/** The name of a kind, for one item of it: `Login`, `SSH key`. */
export function kindName(kind: string): string {
  return text.kinds[kind] ?? kind
}

/** What the person has done with a field. A secret already in the vault starts
 * kept: its value never comes to the page, so there is nothing to show in a box. */
type Change = 'keep' | 'type' | 'generate' | 'remove'

/** One field, as the editor holds it. */
interface Row {
  /** For React: the name for a field of the template or of the item, a counter
   * for one added here, whose name is still being typed. */
  key: string
  name: string
  hint: string | null
  secret: boolean
  multiline: boolean
  /** Whether the generator is offered. */
  generated: boolean
  /** Added here: its name and its secrecy are the person's to say. */
  added: boolean
  /** Whether the item has the field now. */
  had: boolean
  /** What a public field holds now; `null` for a secret, which is never sent. */
  was: string | null
  change: Change
  text: string
  recipe: Recipe
}

function templateRow(field: FieldForm, recipe: Recipe): Row {
  return {
    key: field.name,
    name: field.name,
    hint: field.hint,
    secret: field.secret,
    multiline: field.multiline,
    generated: field.generated,
    added: false,
    had: false,
    was: null,
    change: 'type',
    text: '',
    recipe,
  }
}

/** The fields of a new item of `form`: its template, empty. */
function rowsOfKind(form: KindForm | undefined, recipe: Recipe): Row[] {
  return (form?.fields ?? []).map((field) => templateRow(field, recipe))
}

/** The fields of an item being changed: what it has, in its order, public values
 * as they are and secrets kept; then what its template offers that it has not. */
function rowsOfCard(card: ItemCard, form: KindForm | undefined, recipe: Recipe): Row[] {
  if (card.body.shape !== 'record') return []
  const spec = (name: string) => form?.fields.find((field) => field.name === name)
  const had = card.body.fields.map((field): Row => {
    const known = spec(field.name)
    const common = { key: field.name, name: field.name, hint: known?.hint ?? null, added: false, had: true, recipe }
    if (field.shape === 'plain') {
      return {
        ...common,
        secret: false,
        multiline: known?.multiline ?? field.value.includes('\n'),
        generated: false,
        was: field.value,
        change: 'type',
        text: field.value,
      }
    }
    return {
      ...common,
      secret: true,
      multiline: known?.multiline ?? false,
      // A secret of the item's own that its template does not name is the
      // person's: theirs to have made up. A one-time password key is issued.
      generated: known === undefined ? field.shape === 'secret' : known.generated,
      was: null,
      change: 'keep',
      text: '',
    }
  })
  const names = new Set(card.body.fields.map((field) => field.name))
  const offered = (form?.fields ?? []).filter((field) => !names.has(field.name)).map((field) => templateRow(field, recipe))
  return [...had, ...offered]
}

/** What a field is given: typed, made when saved, or nothing. */
function valueOf(row: Row): Value | null {
  if (row.change === 'generate') return { from: 'generated', recipe: row.recipe }
  if (row.change === 'type') return { from: 'typed', text: row.text }
  return null
}

/** A new item, from what the editor holds. Fields left empty are not sent: they
 * are not there. */
function draftOf(
  form: KindForm,
  title: string,
  tags: string[],
  note: string,
  file: string | null,
  rows: Row[],
): Draft {
  switch (form.shape) {
    case 'note':
      return { title, tags, contents: { shape: 'note', text: note } }
    case 'file':
      return { title, tags, contents: { shape: 'file', path: file ?? '' } }
    case 'record':
      return {
        title,
        tags,
        contents: {
          shape: 'record',
          kind: form.kind,
          fields: rows.flatMap((row): DraftField[] => {
            const value = valueOf(row)
            if (value === null || (value.from === 'typed' && value.text === '')) return []
            return [{ name: row.name, secret: row.added ? row.secret : null, value }]
          }),
        },
      }
  }
}

const sameTags = (a: string[], b: string[]) =>
  a.length === b.length && [...a].sort().every((tag, index) => tag === [...b].sort()[index])

/** What changed in an item, and nothing else: the process applies it on top of
 * the item as the file has it when it is saved. */
function editOf(card: ItemCard, title: string, tags: string[], note: string, rows: Row[]): Edit {
  const fields: FieldEdit[] = []
  for (const row of rows) {
    if (row.change === 'remove') {
      if (row.had) fields.push({ change: 'remove', name: row.name })
      continue
    }
    const value = valueOf(row)
    if (value === null) continue
    if (value.from === 'typed') {
      // A public value as it was, a secret not typed again, an offered field
      // left empty: nothing changed.
      if (row.had && !row.secret && value.text === row.was) continue
      if (value.text === '' && (row.secret || !row.had)) continue
    }
    fields.push({ change: 'set', name: row.name, secret: row.added ? row.secret : null, value })
  }
  return {
    title: title.trim() === card.title ? null : title,
    tags: sameTags(tags, card.tags) ? null : tags,
    text: card.body.shape === 'note' && note !== card.body.text ? note : null,
    fields,
  }
}

function changesSomething(edit: Edit): boolean {
  return edit.title !== null || edit.tags !== null || edit.text !== null || edit.fields.length > 0
}

/**
 * An item, being made or changed.
 *
 * What the form is made of comes from the core's templates (`forms`): the
 * fields of a kind, which are secret, which run to several lines, which the
 * generator is offered for. The page holds what is typed into it and nothing
 * more. A secret already in the vault is not sent here to be edited - it is
 * named, kept unless changed, and replaced by typing a new one or by one the
 * process generates when the item is saved, which this page never sees.
 *
 * What is sent for an item already there is what changed, so a field that the
 * command line or a sync changed meanwhile keeps what was written there.
 */
export function Editor({ forms, editing, onSaved, onCancel, onDirtyChange }: Props) {
  const card = editing.at === 'edit' ? editing.card : null
  const [kind, setKind] = useState(editing.at === 'new' ? editing.kind : editing.card.kind)
  const form = forms.kinds.find((entry) => entry.kind === kind)
  const [title, setTitle] = useState(card?.title ?? '')
  const [tags, setTags] = useState<string[]>(card?.tags ?? [])
  const [note, setNote] = useState(card !== null && card.body.shape === 'note' ? card.body.text : '')
  const [file, setFile] = useState<string | null>(null)
  const [rows, setRows] = useState<Row[]>(() =>
    card === null ? rowsOfKind(form, forms.recipe) : rowsOfCard(card, form, forms.recipe),
  )
  const [added, setAdded] = useState(0)
  const [busy, setBusy] = useState(false)
  const [failure, setFailure] = useState<Failure | null>(null)

  // What a new item is made of, or what an item being changed is, decides the
  // form: a note has text, a record fields, a file a file, and an item of a
  // kind this build does not know only its labels.
  const shape = card === null ? (form?.shape ?? 'record') : card.body.shape

  const dirty =
    card === null
      ? title !== '' ||
        tags.length > 0 ||
        note !== '' ||
        file !== null ||
        rows.some((row) => row.added || row.text !== '' || row.change === 'generate')
      : changesSomething(editOf(card, title, tags, note, rows))
  useEffect(() => onDirtyChange?.(dirty), [dirty, onDirtyChange])

  const update = (key: string, change: Partial<Row>) =>
    setRows((now) => now.map((row) => (row.key === key ? { ...row, ...change } : row)))

  /* Another kind keeps what was typed into the fields both kinds have: a
   * password typed for a login is still there when it turns out to be Wi-Fi. */
  const pickKind = (next: string) => {
    if (next === kind) return
    const nextForm = forms.kinds.find((entry) => entry.kind === next)
    setKind(next)
    setRows((now) => [
      ...rowsOfKind(nextForm, forms.recipe).map((row) => {
        const typed = now.find((was) => !was.added && was.name === row.name)
        return typed === undefined ? row : { ...row, change: typed.change, text: typed.text, recipe: typed.recipe }
      }),
      ...now.filter((row) => row.added),
    ])
  }

  const addField = () => {
    setAdded((count) => count + 1)
    setRows((now) => [
      ...now,
      {
        key: `added-${added}`,
        name: '',
        hint: null,
        secret: true,
        multiline: false,
        generated: true,
        added: true,
        had: false,
        was: null,
        change: 'type',
        text: '',
        recipe: forms.recipe,
      },
    ])
  }

  const chooseFile = async () => {
    const chosen = await ipc.chooseFile(file).catch(() => null)
    if (chosen === null) return
    setFile(chosen)
  }

  const submit = async (event: FormEvent) => {
    event.preventDefault()
    if (busy || form === undefined) return
    setBusy(true)
    setFailure(null)
    try {
      if (card === null) {
        onSaved(await ipc.addItem(draftOf(form, title, tags, note, file, rows)), true)
      } else {
        const edit = editOf(card, title, tags, note, rows)
        onSaved(card.id, changesSomething(edit) ? await ipc.editItem(card.uuid, edit) : false)
      }
    } catch (cause) {
      setFailure(ipc.isFailure(cause) ? cause : unexpected(cause))
      setBusy(false)
    }
  }

  const fileName = file === null ? null : (file.split(/[\\/]/).pop() ?? file)
  const ready = !busy && (shape === 'file' && card === null ? file !== null : title.trim() !== '')

  return (
    <form onSubmit={(event) => void submit(event)} className="flex flex-col gap-4" aria-label={card === null ? text.editor.newTitle : text.editor.editTitle}>
      {card === null ? <KindChoice forms={forms} kind={kind} onPick={pickKind} /> : null}

      <Field label={text.editor.title} required={!(shape === 'file' && card === null)}>
        <Input
          autoFocus
          autoComplete="off"
          spellCheck={false}
          value={title}
          placeholder={shape === 'file' && card === null ? (fileName ?? text.editor.fileTitle) : undefined}
          onChange={(event) => setTitle(event.target.value)}
          className="font-mono"
        />
      </Field>

      {shape === 'note' ? (
        <Field label={text.editor.text}>
          <Textarea
            autoResize
            maxRows={18}
            rows={5}
            spellCheck={false}
            value={note}
            onChange={(event) => setNote(event.target.value)}
            className="font-mono"
          />
        </Field>
      ) : null}

      {shape === 'record' ? (
        <section aria-label={text.editor.fields} className="flex flex-col gap-3">
          {rows.map((row) => (
            <FieldRow
              key={row.key}
              row={row}
              forms={forms}
              onChange={(change) => update(row.key, change)}
              onDrop={() => setRows((now) => now.filter((other) => other.key !== row.key))}
            />
          ))}
          <div>
            <Button type="button" size="sm" variant="ghost" onClick={addField}>
              <Plus aria-hidden />
              {text.editor.addField}
            </Button>
          </div>
        </section>
      ) : null}

      {shape === 'file' && card === null ? (
        <div className="flex flex-col gap-1.5">
          <span className="caption">{text.editor.file}</span>
          <div className="flex items-center gap-2">
            <span className="min-w-0 flex-1 truncate rounded-md border border-line bg-bg px-3 py-1.5 font-mono text-sm text-text" title={file ?? undefined}>
              {fileName ?? <span className="text-faint">{text.editor.noFile}</span>}
            </span>
            <Button type="button" size="sm" onClick={() => void chooseFile()}>
              {text.editor.chooseFile}
            </Button>
          </div>
          <p className="text-xs text-dim">{text.editor.fileHint}</p>
        </div>
      ) : null}
      {card !== null && card.body.shape === 'file' ? <p className="text-sm text-dim">{text.editor.fileLabels}</p> : null}
      {card !== null && card.body.shape === 'unknown' ? (
        <p className="text-sm text-dim">{text.editor.unknownLabels}</p>
      ) : null}

      <Field label={text.editor.tags} help={text.editor.tagsHint}>
        <TagInput value={tags} onValueChange={setTags} removeLabel={text.editor.removeTag} />
      </Field>

      {failure === null ? null : (
        <Alert tone="bad" role="alert">
          {failure.message}
        </Alert>
      )}

      <div className="flex justify-end gap-2 border-t border-line pt-3">
        <Button type="button" disabled={busy} onClick={onCancel}>
          {text.editor.cancel}
        </Button>
        <Button type="submit" variant="primary" disabled={!ready}>
          {busy ? text.editor.saving : text.editor.save}
        </Button>
      </div>
    </form>
  )
}

/** Which kind a new item is. */
function KindChoice({ forms, kind, onPick }: { forms: Forms; kind: string; onPick: (kind: string) => void }) {
  const label = useId()
  return (
    <div className="flex flex-col gap-1.5">
      <span id={label} className="caption">
        {text.editor.kind}
      </span>
      <Select
        value={kind}
        items={forms.kinds.map((entry) => ({ value: entry.kind, label: kindName(entry.kind) }))}
        onValueChange={(next) => {
          if (next !== null) onPick(next)
        }}
      >
        <SelectTrigger aria-labelledby={label} className="w-56">
          <SelectValue />
          <SelectIcon>
            <ChevronDown aria-hidden className="size-3.5 text-dim" />
          </SelectIcon>
        </SelectTrigger>
        <SelectPopup>
          {forms.kinds.map((entry) => (
            <SelectItem key={entry.kind} value={entry.kind}>
              <SelectItemText>{kindName(entry.kind)}</SelectItemText>
            </SelectItem>
          ))}
        </SelectPopup>
      </Select>
    </div>
  )
}

/** One field of a record in the editor. */
function FieldRow({
  row,
  forms,
  onChange,
  onDrop,
}: {
  row: Row
  forms: Forms
  onChange: (change: Partial<Row>) => void
  onDrop: () => void
}) {
  const label = row.added ? (row.name.trim() === '' ? text.editor.fieldName : row.name) : row.name

  return (
    <section aria-label={label} className="flex flex-col gap-1.5">
      {row.added ? (
        <div className="flex items-end gap-2">
          <Field label={text.editor.fieldName} className="min-w-0 flex-1">
            <Input
              autoComplete="off"
              spellCheck={false}
              value={row.name}
              onChange={(event) => onChange({ name: event.target.value })}
              className="font-mono"
            />
          </Field>
          <Switch checked={row.secret} onCheckedChange={(secret) => onChange({ secret, change: 'type' })} className="mb-2">
            {text.editor.fieldSecret}
          </Switch>
          <Button type="button" variant="icon" size="sm" aria-label={text.editor.removeField} onClick={onDrop} className="mb-1">
            <X aria-hidden />
          </Button>
        </div>
      ) : null}
      {row.secret ? (
        <SecretValue row={row} label={label} forms={forms} onChange={onChange} />
      ) : (
        <PublicValue row={row} label={label} onChange={onChange} />
      )}
    </section>
  )
}

/** A public field: its value as it is, to change in place. Emptied, it goes. */
function PublicValue({ row, label, onChange }: { row: Row; label: string; onChange: (change: Partial<Row>) => void }) {
  const help = row.had && row.hint === null ? text.editor.emptyTakesAway : (row.hint ?? undefined)
  return (
    <Field label={label} labelHidden={row.added} help={row.added ? undefined : help}>
      {row.multiline ? (
        <Textarea
          autoResize
          maxRows={12}
          rows={2}
          spellCheck={false}
          value={row.text}
          onChange={(event) => onChange({ text: event.target.value, change: 'type' })}
          className="font-mono"
        />
      ) : (
        <Input
          autoComplete="off"
          spellCheck={false}
          value={row.text}
          onChange={(event) => onChange({ text: event.target.value, change: 'type' })}
          className="font-mono"
        />
      )}
    </Field>
  )
}

/** A secret field: kept, typed, generated or taken away. */
function SecretValue({
  row,
  label,
  forms,
  onChange,
}: {
  row: Row
  label: string
  forms: Forms
  onChange: (change: Partial<Row>) => void
}) {
  // For a secret of several lines, which a password box cannot hold: masked
  // like one until the person asks to see it.
  const [shown, setShown] = useState(false)

  const keep = row.had ? (
    <Button type="button" size="sm" variant="ghost" onClick={() => onChange({ change: 'keep', text: '' })}>
      {text.editor.keep}
    </Button>
  ) : null

  if (row.change === 'keep' || row.change === 'remove') {
    const removed = row.change === 'remove'
    return (
      <Labelled label={label} hidden={row.added} hint={row.hint}>
        <div className="flex min-h-10 flex-wrap items-center gap-2 rounded-md border border-line bg-bg px-3 py-1.5">
          <span
            className={
              removed
                ? 'min-w-0 flex-1 truncate text-sm text-bad line-through'
                : 'min-w-0 flex-1 truncate font-mono text-sm tracking-widest text-dim'
            }
          >
            {removed ? text.editor.removed : text.card.hidden}
          </span>
          {removed ? null : <span className="font-mono text-2xs text-faint">{text.editor.unchanged}</span>}
          {removed ? (
            keep
          ) : (
            <>
              <Button type="button" size="sm" aria-label={`${text.editor.change} ${label}`} onClick={() => onChange({ change: 'type' })}>
                {text.editor.change}
              </Button>
              {row.generated ? (
                <Button
                  type="button"
                  size="sm"
                  aria-label={`${text.editor.generate} ${label}`}
                  onClick={() => onChange({ change: 'generate' })}
                >
                  {text.editor.generate}
                </Button>
              ) : null}
              <Button
                type="button"
                size="sm"
                variant="ghost"
                aria-label={`${text.editor.remove} ${label}`}
                onClick={() => onChange({ change: 'remove' })}
              >
                {text.editor.remove}
              </Button>
            </>
          )}
        </div>
      </Labelled>
    )
  }

  if (row.change === 'generate') {
    return (
      <Labelled label={label} hidden={row.added} hint={null}>
        <div className="flex flex-col gap-3 rounded-md border border-line p-3">
          <Generator recipe={row.recipe} forms={forms} onChange={(recipe) => onChange({ recipe })} />
          <div className="flex gap-2">
            <Button type="button" size="sm" variant="ghost" onClick={() => onChange({ change: 'type', text: '' })}>
              {text.editor.typeInstead}
            </Button>
            {keep}
          </div>
        </div>
      </Labelled>
    )
  }

  return (
    <div className="flex flex-col gap-1.5">
      <Field label={label} labelHidden={row.added} help={row.hint ?? undefined}>
        {row.multiline ? (
          <Textarea
            autoResize
            maxRows={12}
            rows={3}
            spellCheck={false}
            autoComplete="off"
            value={row.text}
            onChange={(event) => onChange({ text: event.target.value })}
            className={shown ? 'font-mono' : 'font-mono [-webkit-text-security:disc]'}
          />
        ) : (
          <PasswordField
            autoComplete="off"
            showLabel={text.editor.show(label)}
            hideLabel={text.editor.hide(label)}
            value={row.text}
            onValueChange={(typed) => onChange({ text: typed })}
            className="font-mono"
          />
        )}
      </Field>
      <div className="flex flex-wrap gap-2">
        {row.multiline ? (
          <Button type="button" size="sm" variant="ghost" aria-pressed={shown} onClick={() => setShown((now) => !now)}>
            {shown ? text.editor.hide(label) : text.editor.show(label)}
          </Button>
        ) : null}
        {row.generated ? (
          <Button
            type="button"
            size="sm"
            aria-label={`${text.editor.generate} ${label}`}
            onClick={() => onChange({ change: 'generate', text: '' })}
          >
            {text.editor.generate}
          </Button>
        ) : null}
        {keep}
      </div>
    </div>
  )
}

/** A caption over something that is not one control: a kept secret, a generator. */
function Labelled({ label, hidden, hint, children }: { label: string; hidden: boolean; hint: string | null; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-1.5">
      <span className={hidden ? 'sr-only' : 'caption'}>{label}</span>
      {children}
      {hint === null || hidden ? null : <span className="text-xs text-dim">{hint}</span>}
    </div>
  )
}

/** How a secret is to be made, and what that would be worth. Nothing is made
 * here: the process makes it when the item is saved. */
function Generator({ recipe, forms, onChange }: { recipe: Recipe; forms: Forms; onChange: (recipe: Recipe) => void }) {
  const [worth, setWorth] = useState<Worth | null>(null)
  const [refused, setRefused] = useState<string | null>(null)
  const asked = JSON.stringify(recipe)

  useEffect(() => {
    let live = true
    ipc
      .worth(JSON.parse(asked) as Recipe)
      .then((found) => {
        if (!live) return
        setWorth(found)
        setRefused(null)
      })
      .catch((cause: unknown) => {
        if (!live) return
        setWorth(null)
        setRefused(ipc.isFailure(cause) ? cause.message : String(cause))
      })
    return () => {
      live = false
    }
  }, [asked])

  const length = recipe.make === 'words' ? (forms.recipe.make === 'characters' ? forms.recipe.length : 20) : recipe.length
  const make = (next: string) => {
    if (next === recipe.make) return
    if (next === 'words') onChange({ make: 'words', count: forms.words, list: 'en', separator: '-' })
    else if (next === 'pronounceable') onChange({ make: 'pronounceable', length })
    else onChange(forms.recipe.make === 'characters' ? { ...forms.recipe, length } : forms.recipe)
  }

  return (
    <div className="flex flex-col gap-3">
      <SegmentedControl aria-label={text.generator.how} value={recipe.make} onValueChange={make}>
        <Segment value="characters">{text.generator.characters}</Segment>
        <Segment value="pronounceable">{text.generator.pronounceable}</Segment>
        <Segment value="words">{text.generator.words}</Segment>
      </SegmentedControl>

      {recipe.make === 'words' ? (
        <div className="flex flex-wrap items-end gap-3">
          <Field label={text.generator.count}>
            <NumberField
              value={recipe.count}
              min={1}
              max={forms.maxLength}
              onValueChange={(count) => onChange({ ...recipe, count: count ?? 0 })}
              className="w-32"
            />
          </Field>
          <SegmentedControl
            aria-label={text.generator.list}
            value={recipe.list}
            onValueChange={(list) => onChange({ ...recipe, list: list === 'ru' ? 'ru' : 'en' })}
          >
            <Segment value="en">{text.generator.english}</Segment>
            <Segment value="ru">{text.generator.russian}</Segment>
          </SegmentedControl>
          <Field label={text.generator.separator}>
            <Input
              autoComplete="off"
              spellCheck={false}
              value={recipe.separator}
              onChange={(event) => onChange({ ...recipe, separator: event.target.value })}
              className="w-24 font-mono"
            />
          </Field>
        </div>
      ) : (
        <div className="flex flex-wrap items-end gap-4">
          <Field label={text.generator.length}>
            <NumberField
              value={recipe.length}
              min={1}
              max={forms.maxLength}
              onValueChange={(next) => onChange({ ...recipe, length: next ?? 0 })}
              className="w-32"
            />
          </Field>
          {recipe.make === 'characters' ? (
            <div className="flex flex-wrap gap-4 pb-2">
              <Switch checked={recipe.uppercase} onCheckedChange={(uppercase) => onChange({ ...recipe, uppercase })}>
                {text.generator.uppercase}
              </Switch>
              <Switch checked={recipe.digits} onCheckedChange={(digits) => onChange({ ...recipe, digits })}>
                {text.generator.digits}
              </Switch>
              <Switch checked={recipe.symbols} onCheckedChange={(symbols) => onChange({ ...recipe, symbols })}>
                {text.generator.symbols}
              </Switch>
            </div>
          ) : null}
        </div>
      )}

      <div className="flex flex-col gap-1" role="status">
        {refused !== null ? (
          <span className="text-xs text-bad">{refused}</span>
        ) : worth === null ? null : (
          <span className={worth.offline ? 'font-mono text-xs text-text' : 'font-mono text-xs text-warn'}>
            {text.generator.worth(worth.bits)}
            {worth.offline ? '' : ` · ${text.generator.weak}`}
          </span>
        )}
        <span className="text-xs text-dim">{text.generator.made}</span>
      </div>
    </div>
  )
}
