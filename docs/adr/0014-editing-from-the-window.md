# ADR-0014: Editing from the window

- **Status:** accepted
- **Date:** 2026-11-11

## Context

The window could show an item, copy from it, bring back an earlier version and
sync the vault, but adding or changing an item meant going to the command line.
The approved design has **New item**, **Edit** and **Delete** on the vault screen.

Three things make editing in a window harder than showing:

- The page is a webview, and a string in it cannot be wiped (ADR-0011). Until now a
  secret reached the page only when someone pressed Show for that one field.
- An editor stays open while a person types, and the file can move on under it: the
  command line writes through the window's keys, a sync client replaces the file
  (ADR-0013).
- A form that already exists in the command line - which fields a kind has, which
  are secret, what a value must look like - must not be decided a second time on the
  page.

## Decision

### The form is drawn from the core's templates

`forms` hands the page every kind this build can make, with each field of its
template: its name, its help text, whether it is secret, whether its value runs to
several lines, and whether the generator is offered for it. The page draws nothing
it was not told. A field added to a template reaches the window with nothing written
on the page.

Whether a value runs to several lines (a key as it is in its file, notes) and whether
it may be generated became part of `FieldSpec`, so that the command line and the
window read one table rather than two.

### A secret already in the vault is not sent to the page to be edited

Opening an item to edit it sends what the card sends (ADR-0011): public values, and
every secret by name only. A secret starts **kept**. The person can type a new one,
have one generated, or remove the field; until they do, the value stays in the
process and is not touched.

What is typed is in the page, as anything typed in a form is. That is the cost of
typing a secret, the same as at Unlock, and it is paid only for the value being
typed.

### A generated secret is made in the process, when the item is saved

The page sends a recipe - the options of `sefy gen` - and nothing comes back but what
the recipe is worth: `Recipe::bits`, counted before anything is drawn, by the same
arithmetic the generator reports after. The value is made by `sefy_core::generate`
when the item is saved, and reaches the page only through Show, or the clipboard
through Copy, like any other secret. The default recipe is the one `sefy gen` uses
without options, from one constant.

The generator is offered for a secret a person would otherwise choose
(`AuditAs::Password`: a login's password, a Wi-Fi password, a key's passphrase), and
for a secret field the person adds. It is not offered for what someone else issues
(a card number, a token, a private key, a one-time password key): a value made up
here would be one nobody accepts.

The alternative was to generate on request and show the value in the box before
saving, as most password managers do. That puts every generated password in the page
whether or not anyone looks at it, to show a string whose only property worth
checking is its length and its classes - which the recipe already says, and the bits
say better. Rejected.

Generating a password in place of an existing one is a **rotation**: the one it
replaces becomes a version in the item's history (ADR-0005), where the site's "current
password" box can be filled from.

### An edit is what changed, applied on top of the file as it is

The page sends only what the person changed: a new title, new tags, a note's new
text, and for a record each field set, generated or removed. The process reads the
file again before writing (ADR-0013) and applies those changes to the item as the
file has it at that moment. A field the command line or a sync changed meanwhile,
which nobody touched here, keeps what was written there; one that was touched here is
replaced, and what it replaced is in the history.

The rules of a field change are the command line's, and live in the core:
`model::set_field` keeps a field in its place and its secrecy, and a new field takes
the secrecy its template gives the name; `normalize_value` holds a one-time password
key and an expiry date to their shape. An empty value is a field that is not there,
as in `sefy add`. A record cannot be left without any field, a note has no fields,
and a file or an item of a kind this build does not know can only be retitled and
retagged.

The alternative was to send the whole item back. A form opened before the command
line wrote would then erase that write with its own stale copy, which is exactly the
loss ADR-0013 exists to prevent. Rejected.

### Writes go by identity

`edit_item` and `delete_item` name the item by its uuid, not by its id. An id is a
row, and a row can be given to another item once its own is removed - by the command
line, between one look at the file and the next. What is changed or removed has to be
the item the person was looking at. An item that is gone is refused for an edit, and
counts as removed for a delete.

### Delete asks as `sefy rm` asks

The question says how many earlier versions go with the item, that there is no trash,
and that a copy of the vault elsewhere still holds it: a merge cannot tell "removed
over there" from "added over here" and never removes, so a sync brings it back
unless it is removed there too.

### A file is read by the process

A new file item takes a path from the system's file dialog (ADR-0010) and is read when
it is saved. Its bytes never pass through the page. Taking one out stays with
`sefy extract`.

## Consequences

- Everything the command line adds or changes can be added or changed in the window,
  except a file's contents and one field restored alone from history
  (`sefy restore --field`).
- A rotated password is never on the screen unless someone asks to see it, and the
  old one stays a click away in the history.
- `FieldSpec` has two more properties; nothing about the format, the schema or what
  older builds read changes.
- What a person types into the editor is in the page until the editor closes. The
  window's form autofill stays off (ADR-0010), so none of it is kept by the webview.
