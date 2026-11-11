import type { ReactNode } from 'react'
import { NumberField as Base } from '@base-ui/react/number-field'
import { cn, useLocale } from 'dowel-ui'
import { fieldClasses } from './input'

/*
 * NumberField - a number, and the two ways of changing it.
 *
 * A number typed into a text input is a string that happens to look like a
 * number, and every product then writes the same four fixes: strip the
 * letters, clamp to a range, round to a step, and decide what an empty box
 * means. This is those four, once, plus the stepper - because a value with a
 * small range is faster nudged than typed.
 *
 * Base UI carries the parts that are genuinely hard: the arrow keys with
 * PageUp/PageDown for the large step, the parse of what a person actually
 * types (spaces, a comma for a decimal point, a pasted currency string), and
 * `Intl.NumberFormat` for how it reads back. That last one matters more than
 * it looks: a number field that shows `1234.5` where the reader writes
 * `1 234,5` is a field they have to translate in their head.
 *
 * `unit` is ours, and it is a label rather than part of the value. Putting
 * "px" inside the input makes it something to parse and something to delete
 * by accident; beside the input it is a caption that cannot be typed into.
 * The value stays a number.
 *
 * Empty is `null`, not zero. "No number" and "the number zero" are different
 * facts - a price of nothing and no price yet - and a field that returns 0 for
 * an empty box makes them the same the moment it is saved.
 */

export interface NumberFieldProps {
  /** The controlled number. `null` is an empty box. */
  value?: number | null
  /** The starting number of an uncontrolled field. */
  defaultValue?: number
  /** Called with the new number as it changes, or `null` when the box is
   * emptied. */
  onValueChange?: (value: number | null) => void
  /** The lowest the number may be. The arrows stop here, and a typed number
   * below it moves to it on blur unless `allowOutOfRange` is set. */
  min?: number
  /** The highest the number may be. See `min`. */
  max?: number
  /** Keep a typed number outside `min`/`max` as typed, rather than moving it
   * to the bound on blur - for a form that says what is wrong instead. A 9
   * typed for an age of at least 13 became 13 without a word; the arrows
   * still stop at the bounds. */
  allowOutOfRange?: boolean
  /** What the arrows change it by. */
  step?: number
  /** What PageUp and PageDown change it by, when a single step is too slow. */
  largeStep?: number
  /** How the number reads: `Intl.NumberFormat` options, so a currency or a
   * percentage is a prop rather than a wrapper. */
  format?: Intl.NumberFormatOptions
  /** Which conventions `format` follows. Left alone it is the application's
   * language (see `useLocale`); a product states one only when the figure
   * belongs to a place rather than to a person - a price in a fixed market. */
  locale?: string
  /** What the number is in - `px`, `kg`, `%`. A caption beside the field, not
   * part of the value. */
  unit?: ReactNode
  /** Hide the stepper. For a field with a wide range, where the buttons are
   * an invitation to click sixty times. */
  hideStepper?: boolean
  /** Turns the field off: it cannot be focused or changed. */
  disabled?: boolean
  /** The number can be read and selected but not changed. */
  readOnly?: boolean
  /** Marks the field as required, for a form to check. */
  required?: boolean
  /** The name the number is submitted under in a form. */
  name?: string
  /** Shown in the box while it is empty. */
  placeholder?: string
  /** Names the field for a screen reader, when there is no visible label. */
  'aria-label'?: string
  className?: string
}

/** The stepper's two buttons. Square, the height of the field, and marked
 * `aria-hidden` because the input they belong to already announces its value
 * and its range - a screen reader hearing "increase, decrease" as separate
 * controls learns nothing it did not have. */
const stepperButton = cn(
  'flex w-7 shrink-0 items-center justify-center text-dim',
  'transition-colors hover:bg-soft hover:text-text',
  'disabled:pointer-events-none disabled:opacity-50',
)

export function NumberField({
  unit,
  hideStepper = false,
  className,
  placeholder,
  'aria-label': ariaLabel,
  locale,
  ...props
}: NumberFieldProps) {
  const language = useLocale(locale)
  /* What a `Field` hands its control - the id and the `aria-` references - is
   * already on the input, put there by Base UI from the field it sits in.
   * Spread onto the root as well, it landed on a `<div>` with no role, where
   * `aria-labelledby` and `aria-invalid` mean nothing and axe asks a person
   * to look (aria-prohibited-attr). */
  const root: Record<string, unknown> = { ...props }
  for (const key of ['id', 'aria-labelledby', 'aria-describedby', 'aria-invalid']) delete root[key]
  return (
    <Base.Root {...root} locale={language} className={cn('inline-flex items-center gap-2', className)}>
      <Base.Group
        className={cn(
          fieldClasses,
          'flex h-control items-stretch overflow-hidden p-0',
          // The group carries the field's clothes, so the focus ring belongs
          // to the whole control rather than to the bare input inside it.
          'focus-within:outline-2 focus-within:outline-offset-0 focus-within:outline-accent',
        )}
      >
        {!hideStepper && (
          <Base.Decrement className={cn(stepperButton, 'border-r border-line')} aria-hidden>
            <svg viewBox="0 0 16 16" className="size-3.5">
              <path d="M4 8h8" stroke="currentColor" strokeWidth="1.75" strokeLinecap="round" />
            </svg>
          </Base.Decrement>
        )}

        <Base.Input
          placeholder={placeholder}
          aria-label={ariaLabel}
          className={cn(
            // 16px under a finger, for the reason `fieldClasses` gives.
            'w-full min-w-0 bg-transparent px-2.5 text-sm pointer-coarse:text-lg text-text placeholder:text-faint',
            'outline-none',
            // Figures line up in a column, which is the whole reason a number
            // is in a field of its own.
            'tabular-nums',
            hideStepper ? 'text-left' : 'text-center',
          )}
        />

        {!hideStepper && (
          <Base.Increment className={cn(stepperButton, 'border-l border-line')} aria-hidden>
            <svg viewBox="0 0 16 16" className="size-3.5">
              <path d="M8 4v8M4 8h8" stroke="currentColor" strokeWidth="1.75" strokeLinecap="round" />
            </svg>
          </Base.Increment>
        )}
      </Base.Group>

      {unit !== undefined && <span className="shrink-0 text-xs text-dim">{unit}</span>}
    </Base.Root>
  )
}
