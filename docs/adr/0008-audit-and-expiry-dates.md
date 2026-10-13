# ADR-0008: The audit, and the date a record stops working

- **Status:** accepted
- **Date:** 2026-10-13

## Context

By 0.14 a vault held everything needed to answer "what in here needs looking
at": every secret, the history of every value since 0.12 (ADR-0005), an offline
strength estimate since 0.9. Nothing asked the question. Two things were also
missing for it to be answered well:

1. **What a secret is.** A login's password and a card's CVV are both secret
   fields. One was chosen by a person and can be weak, reused and stale; the
   other is three digits printed by a bank. Checking every secret the same way
   would bury the real findings under every PIN in the vault.
2. **When a record stops working.** An API token had an `expires` field of free
   text; a card had `expiry`, "as printed". Neither could be counted from, and
   the two names meant one thing.

The invariants hold as ever: the plaintext never touches the disk (ADR-0001),
and nothing about the vault goes anywhere it was not sent (ADR-0002).

## Decision

### The template says what a field is audited as

`FieldSpec` gains `audit`: **Password** (chosen by a person: a login's and a
Wi-Fi network's `password`, an SSH key's `passphrase`), **Unique** (issued by
someone: a card's `number`, a `token`, a `private-key`, an `account`, a `totp`
key), or **Nothing** (a public field, a PIN, a CVV). A new kind is audited by
its row in the table, with no change to the audit.

A field outside its kind's template takes the role a field of that name has in
the templates that list it, provided it is secret: a `password` added to a bank
record is a password. A unit test holds every name to one role across all
templates, which is what makes that lookup a single answer.

### What is looked for

- Passwords: **weak** (zxcvbn below 3 of 4, with the record's title and public
  fields as words the password must not lean on), **reused** (the same value in
  more than one item), **old** (unchanged for over a year).
- Issued secrets: **duplicated** (the same value, under the same field name, in
  more than one item — almost always a copy left behind).
- Dates: **expired**, **expiring** (30 days or fewer), **unreadable**.

Values are compared through SHA-256 fingerprints with the class in front, so the
report never holds a value and a card number never matches an account number of
the same digits. Thresholds are constants, not options: an audit that asks how
strict to be is a setting standing in for a choice.

A password's age is when it took its current value, not when the item last
changed. The audit walks the item's line of versions back from the current one
while the field held the same value; versions that lost a merge (ADR-0005) are
not part of the line, since they were never what the item said.

### The report names places and never values

An id, a title, a field, and what is wrong. Not the value, and not zxcvbn's
explanation either: "a date is easy to guess" is a statement about what the
password contains. The exit status is `1` when anything is found, so a scheduled
audit is a check.

### Nothing is looked up

No breach list, not even a k-anonymous hash prefix. A feature that sends
anything derived from a password off the machine is the first thing in sefy that
would; an audit that says "nothing found" while a strong password sits in
someone else's breach is the limit, and the threat model says so.

### `expires` is a field, in two shapes

The date lives in the record's contents, as a field named `expires`, on every
kind. Not as a label beside the title and tags: a date belongs to the secret it
came with — a new token brings a new one, a renewed card a new one with its new
CVV — and bringing back an earlier version must bring that version's date back
with it, which only contents do. It travels through merges, history and every
export with nothing added for it.

Stored as `YYYY-MM-DD`, the last day it works, or `YYYY-MM`, through the end of
that month — a card's shape. Input also takes a card's printed `MM/YY` and
`MM/YYYY` and a full RFC 3339 moment; day-first and month-first dates with
slashes are refused rather than guessed at. A value that is none of these is
refused on the way in from `add` and `edit`; one arriving in an import is kept
as it came, and the audit names it.

A card's field is renamed from `expiry` to `expires`: one name for one thing
across every kind. `fill` hands a card's month to a form as printed, `MM/YY`,
the way it hands over a one-time code rather than its key.

The old name is **read, never written**, like the `credential` kind before it:
a card whose `expiry` reads as a date comes back with it under `expires`, in
place, whether from a row of the database, a kept version or an export. No row
is rewritten, so a row an older build writes after this one has opened the
vault reads the same way. A printed date that is not one — a year on its own —
stays under the old name as an extra field, and the audit names it.

KeePass has an expiry of its own beside an entry's strings. A date sefy can
count from becomes that expiry on export, so KeePass marks the entry when the
day comes; on import, an entry's expiry becomes `expires` unless a string of
that name says it more exactly — a card's month rather than its last day.

### Warnings where a secret is taken

`get`, `open`, `fill`, `otp` and `run` say on stderr when the record is past
its last day or within 30 days of it — when it matters, and where a pipe does
not receive it.

## Consequences

- The vault format and the schema are unchanged. A vault written by 0.15 opens
  in 0.14, which shows a card's `expires` as an extra field and does not fill
  it; a card 0.14 writes into a vault 0.15 has opened reads correctly in 0.15.
- `sefy add card --expiry` is now `--expires`.
- Notes and files cannot state a date: they have no fields. A certificate kept
  as a file does not expire in sefy's eyes.
- A strong password used once that leaked elsewhere is not found. That is the
  price of an audit that never leaves the machine, and the threat model says so.
