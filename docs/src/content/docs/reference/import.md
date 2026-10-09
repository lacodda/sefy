---
title: "import"
description: Add what another sefy, KeePass, Bitwarden or a browser exported to this vault.
---

Adds what another sefy, a password manager or a browser exported to this
vault, reading stdin when no path is given. The format is told from the file
itself; there is nothing to choose.

## Usage

```console
$ sefy import bitwarden_export.json
imported 7 items from a Bitwarden JSON export: 2 notes, 2 logins, 1 card, 1 ssh-key, 1 bank
with 3 earlier versions kept in their history
not imported:
  "deleted" - in the trash
  "from the future" - a Bitwarden item of type 99, which sefy does not know
imported in part:
  "mail" - its passkey stays in Bitwarden: a passkey cannot be exported
  "mail" - its linked field "Linked" is left out: it only points at another field
imported in another shape:
  "shed" - a note with fields of its own; they are added to its text
  "me" - an identity, which sefy has no kind for; kept as a note listing its fields
  "steam" - its one-time password key is not one sefy can read, kept as the field "otp"
bitwarden_export.json still holds all of it in the clear; delete it once the import looks right
```

```sh
sefy --vault ./old.bak export --i-know-this-writes-plaintext \
  | sefy --vault ./new.bak import
```

## What it reads

| File | Where it comes from |
| --- | --- |
| a sefy export | [`sefy export`](/sefy/reference/export/) |
| a KeePass XML export | KeePass 2: File → Export → KeePass XML (2.x). KeePassXC: `keepassxc-cli export --format xml` |
| a Bitwarden JSON export | Tools → Export vault → `.json` in the apps, or `bw export --format json` |
| a CSV of passwords | Chrome, Edge, Brave, Opera, Vivaldi, Firefox, Safari, and the CSV of most password managers |

The file must be UTF-8. Bitwarden's `.json (Encrypted)` export and a KeePass
XML whose values are still sealed with the database's key are refused, with
how to export them openly; anything that is none of the above is refused
outright, before anything is added.

## How entries map

**Bitwarden.** Logins, secure notes, cards, SSH keys and bank accounts become
`login`, `note`, `card`, `ssh-key` and `bank`. An identity, a driver's license
and a passport have no sefy kind; each becomes a note listing every field it
had. Folders and collections become tags, custom fields become fields - hidden
ones secret - and a login's second address becomes `url-2`.

**KeePass.** `UserName`, `Password`, `URL` and `Notes` become a login's
`login`, `password`, `url` and `notes`; an entry holding nothing but notes
becomes a note. Every other string becomes a field, secret when KeePass
protects it in memory. KeePass 2's one-time password settings
(`TimeOtp-*`) and KeePassXC's `otp` both become the `totp` field. An entry's
group becomes a tag holding its path, `Internet/Shops`, and its own tags stay
tags. Each attachment becomes a file item of its own - or the entry itself,
when an attachment is all the entry holds. An entry that went to KeePass from
sefy comes back as the kind it left as.

KeePassXC's XML export names an entry's attachments but leaves their bytes
out. The entry still arrives; each attachment is listed under "imported in
part", to be saved with `keepassxc-cli attachment-export` and added with
[`sefy add file`](/sefy/reference/add/). KeePass 2's own XML export carries
them.

**CSV.** Columns are recognised by name, not by which program wrote them:
`name` or `title`, `url` or `login_uri`, `username` or `login_username`,
`password`, `note` or `notes`, `totp` or `otpauth`, `folder` or `grouping` for a
tag. A row without a name is called after its site. A column sefy does not
recognise becomes a secret field, so nothing in the file is lost; columns that
are a browser's own bookkeeping, like Firefox's `httpRealm`, are left out and
listed.

A one-time password key sefy cannot read - a Steam key, say - is kept as a
secret field named `otp` rather than dropped.

## What it says

Every entry that did not come across exactly as it was is named, under one of
three headings:

- **not imported** - in a trash or recycle bin, of a kind this version of sefy
  does not know, or with nothing in it but a name;
- **imported in part** - something it carried cannot move: a passkey, a
  Bitwarden linked field;
- **imported in another shape** - an identity as a note, an attachment as a
  file item, a key sefy cannot read as an `otp` field.

When the file was read from a path, the last line reminds you that it is still
on disk in the clear.

## Already here, left alone

An entry carries the identity its item had where it came from: sefy's own,
a KeePass entry's UUID, a Bitwarden item's id, Firefox's `guid`. An entry whose
identity is already here is **skipped**, so importing the same file twice does
not double anything:

```console
$ sefy import backup.json
imported 0 items from a sefy export
6 items already here, left alone
```

What was said about those items the first time is not said again; what was
left behind still is.

Skipped means *untouched*, not updated. An export is a snapshot, and it may
easily be older than what is in the vault now - overwriting a password with one
from last month is exactly the kind of quiet damage worth refusing. To bring
newer contents across from another sefy, use [`merge`](/sefy/reference/merge/),
which compares the versions on both sides and keeps whatever it does not choose
in the item's [history](/sefy/reference/history/).

Entries **without** an identity are always added. Chrome's CSV carries none,
nor do exports from sefy 0.1.x or JSON written by hand - there is nothing to
recognise them by, and matching on titles instead would silently collapse two
accounts that happen to share a name.

## History comes along

Earlier contents the other program kept arrive as the item's
[history](/sefy/reference/history/), so a password changed last spring is one
[`restore`](/sefy/reference/restore/) away here too:

- a KeePass entry's history becomes its versions, dated as KeePass dated them;
- Bitwarden's password history - the last few passwords of an item, and the
  last values of its hidden fields - becomes versions that each differ from the
  next in that one value, dated by when it was replaced.

A snapshot that changed nothing in the contents - KeePass keeps one for a
renamed title - is not a version. A sefy export written with
`--with-history` brings its versions as they were, machine names and all.

## All or nothing

The whole file is read and checked before anything is inserted, so a malformed
entry halfway down cannot leave a half-imported vault behind. Either every item
lands or none does.

Entries that sefy cannot store are the deliberate exception: they are named
under "not imported" rather than failing the file. One entry from a newer sefy,
or of a type Bitwarden added last month, should not keep the other nine hundred
out. See [Versions and compatibility](/sefy/concepts/versions/).

## A copy before it writes

An import that adds anything keeps the vault as it was beside it first, as
`FILE.1` - the same copy a merge keeps. Importing the wrong file is then undone
by putting the copy back, not by removing items one at a time. An import that
adds nothing writes nothing. See
[A copy before every merge](/sefy/guides/syncing/#a-copy-before-every-merge).

## Related

- [`export`](/sefy/reference/export/) - writing a file, and the formats
- [Moving between password managers](/sefy/guides/password-managers/) - arriving and leaving
- [`merge`](/sefy/reference/merge/) - folding in another vault, newer contents and all
- [Moving a vault between machines](/sefy/guides/moving-a-vault/) - how copies drift
