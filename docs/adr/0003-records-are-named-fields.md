# ADR-0003: A record is a set of named fields

- **Status:** accepted
- **Date:** 2026-09-01

## Context

Up to 0.6.0 a vault held three kinds of item, and each one was its own
construction all the way down: a `credentials` table with five fixed columns, a
`Payload::Credential(Credential { .. })` variant, a `Field` enum on the command
line naming exactly `password`, `login`, `url` and `totp`, and a branch in every
function that reads a payload.

That shape prices every further kind the same way. A payment card would need a
`cards` table, a payload variant, four more values in the `--field` enum and a
branch in `get`, `show`, `edit`, `export`, `import` and `merge`. An SSH key
would need the same again. The backlog has `sefy type` — user-defined kinds —
which under this shape is not a feature but a rewrite.

The plan for this stage said it in one line: **the fields live inside the
value**. This ADR is what that turns into.

## Decision

### A record is an ordered list of `(name, value, secret)`

`Payload::Fields { kind, fields }` replaces `Payload::Credential`. One table,
`fields (item_id, name, value, secret, position)`, holds every record in the
vault regardless of kind. `notes` and `files` are untouched: a note's body *is*
the item and a file is bytes, so neither is a set of fields and pretending
otherwise would buy nothing.

`position` is stored rather than derived. The order a record reads in is part
of what it is — who, then the secret, then where — and a field the template
never heard of still has to sit somewhere stable.

### Secrecy travels with the value, not with the name

Each stored field carries its own `secret` flag rather than having one looked up
in a template at read time. Two reasons, and the second is the one that decides
it:

- a field the template does not know — added by `edit --set`, or written by a
  future sefy — is still hidden when it should be;
- a record written by another build keeps the secrecy it was written with. A
  lookup would let *this* build's opinion silently reclassify someone else's
  secret as printable.

### A kind is a template, not a type

`ItemKind::Login`, `Card` and `SshKey` each name a `Template`: an ordered list
of `FieldSpec { name, description, secret }`. The template decides what `add`
offers, what secrecy a new field gets by default, and which field `get` takes
when none is named — the kind's first secret one.

A template **does not constrain what a record may hold.** `edit --set` can add a
field no template mentions, and it is stored, shown and exported like any other.
A store that drops what it was handed is worse than one that is untidy, and the
alternative — refusing the field — would make sefy the arbiter of what a user's
own record is allowed to contain.

This is what makes the next kind cheap. `card` and `ssh-key` in this release are
two entries in a static array; they needed no table, no payload variant and no
new branch anywhere. `sefy type` becomes a way to write that array at runtime
rather than a rewrite.

### `--field` is a name, not an enum

`sefy get --field <NAME>` takes any string. The closed enum could not survive
user-defined fields, and a name that the record does not carry is answered by
listing the names it does — which is more useful than a shell completion of four
fixed words ever was.

### `credential` is renamed to `login`

Beside `card` and `ssh-key`, the old name read as the odd one out: a credential
is the category all three belong to, not one of them. `credential` still
**parses** — in vaults, in exports, as `--kind credential` and as
`sefy add credential` — so nothing written before this release stops working.
It is never **written**: a vault touched by 0.7.0 says `login` everywhere.

## Migration

The database schema inside the blob goes from 2 to 3. **The vault file format
stays v1** — this is a change under the envelope, not to it, so nothing about
what the file looks like from outside is affected.

On open, every `credentials` row becomes fields in template order, an absent
optional producing no field rather than an empty one, and `kind = 'credential'`
becomes `kind = 'login'`.

Two decisions about the old table:

**The pass runs on every open, not behind a version check.** A vault travels,
and 0.6.0 on the other machine writes into `credentials` long after this build
migrated the database. `user_version = 3` therefore means "this database passed
through the migration once", never "every row has been brought along". This is
the same lesson the uuid migration taught in 0.2.0, learned there the hard way,
and the gate in `schema_migration.rs` fails if the pass is made conditional.

**The table stays; its rows do not.** Dropping the table would make an older
build's own writes fail outright instead of being carried across on the next
open. But leaving a row in it would let 0.6.0 hand out a password through `get`
— which reads that table directly, without consulting the item's kind — for an
item the same build's `show` and `ls` call a kind they do not understand. One
binary, two answers to "do you know this item". Emptying the table after the
contents are safely in `fields` makes the older build wrong about the item
**consistently**, which is the honest half of the two.

Both halves were found by running the published 0.6.0 binary against a vault
this build had touched, not by reasoning about the code.

## Consequences

- Adding a kind costs a template. `sefy type` is now a plausible feature rather
  than a rewrite, and `gen --save`, TOTP and the importers planned for
  v0.9–v0.13 write against fields instead of waiting for a shape.
- Secret field values are not searchable. They never were — `find` matched a
  credential's login, url and notes but never its password — and the rule is now
  stated once in SQL (`f.secret = 0`) rather than implied by which columns a
  query happened to list.
- An export carries `fields` and, for the names a login used to have, a flat
  copy under the old keys. Readers should prefer `fields`; the flat copy is
  what another tool finds where it expects it. Import prefers `fields` and falls
  back to the flat keys, so an export written by 0.6.0 still imports.
- A vault touched by 0.7.0 has login items 0.6.0 cannot read. That is the
  forward-compatibility contract from 0.6.0 working as designed — the item is
  listed, exported and synced, and the older build says to upgrade — but it is a
  real cost of the rename, and it is why the rename happens here rather than
  after 1.0.
