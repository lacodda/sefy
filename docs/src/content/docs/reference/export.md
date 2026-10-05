---
title: "export"
description: Write the vault's contents out, unencrypted, as sefy JSON, KeePass XML or CSV.
---

Writes the whole vault out, **unencrypted**, in a form another sefy or another
password manager can read. This exists so a vault is never a trap: contents
can be migrated, kept in another form, or moved to a different tool.

## Usage

```sh
sefy export --i-know-this-writes-plaintext [OPTIONS]
```

| Option | Meaning |
| --- | --- |
| `-o, --output <PATH>` | Where to write it; omit to print to stdout. |
| `--format <FORMAT>` | `sefy` (the default), `keepass` or `csv` - see below. |
| `--with-history` | Include each item's earlier versions. Not with `csv`. |
| `--i-know-this-writes-plaintext` | Required. |
| `--force` | Overwrite the destination if it exists. |

```console
$ sefy export --i-know-this-writes-plaintext -o backup.json
wrote backup.json in the clear: 14 items
```

```sh
sefy export --i-know-this-writes-plaintext | gpg -c > backup.json.gpg
```

What the export could not hold goes to stderr either way, so a pipe receives
the file and nothing else.

## Why the flag is required

The acknowledgement flag is required rather than a printed warning: a warning
arrives after the file is already on disk, and scripts do not read them at all.

The resulting file is exactly as sensitive as the vault and protects nothing.
Every password, note and stored file is in it in the clear. Write it somewhere
that is not synced or backed up, and delete it when you are done - or pipe it
straight into whatever consumes it, so it never reaches disk.

## Formats

| `--format` | Holds | Read by |
| --- | --- | --- |
| `sefy` | every kind, every field, identities and times | [`sefy import`](/sefy/reference/import/) |
| `keepass` | every kind, every field, identities and times | KeePass 2, KeePassXC, Bitwarden |
| `csv` | logins only | every browser, most password managers |

To leave sefy for another program, `keepass` is the one that loses nothing a
password manager can hold. `csv` is for a browser.

### sefy

```json
{
  "sefy_export": 1,
  "items": [
    { "uuid": "5f2b…", "title": "bank", "kind": "note", "tags": ["money"],
      "created_at": 1709164800, "updated_at": 1709251200,
      "version": { "uuid": "c41d…", "seq": 2, "made_at": 1709251200, "device": "laptop" },
      "text": "code 4815" },
    { "uuid": "9c14…", "title": "mail", "kind": "login",
      "fields": [
        { "name": "login", "value": "someone" },
        { "name": "password", "value": "…", "secret": true },
        { "name": "url", "value": "…" },
        { "name": "totp", "value": "…", "secret": true },
        { "name": "notes", "value": "…" }
      ],
      "login": "someone", "password": "…", "url": "…", "totp": "…", "notes": "…" },
    { "uuid": "a077…", "title": "key", "kind": "file", "filename": "id_ed25519",
      "bytes_base64": "…" }
  ]
}
```

Notes need `text`; records need `fields`; files need `filename` and
`bytes_base64`. Everything else is optional. This is a plain enough shape to
generate from another tool by hand.

A `card`, an `ssh-key` and a `login` all export their fields as one array of
`{ name, value, secret }` entries - `secret` says whether the field is one
`sefy get` hides by default, not whether this particular export left it out.
A `login` also writes the old flat keys `login`, `password`, `url`, `totp` and
`notes` alongside, for tools - and sefy before 0.7.0 - that only know that
shape. [`import`](/sefy/reference/import/) prefers `fields` when both are
present.

`uuid` is the identity the item had in the vault it came from; `version` is
the identity of its current contents. Together they let an export imported
into another vault be [merged](/sefy/reference/merge/) with this one later as
the same item at the same version, rather than as a stranger or a conflict.
Leave all three out when writing an export by hand - an entry without them is
simply added.

### KeePass XML

The XML KeePass 2 imports under "File → Import → KeePass XML (2.x)", and
KeePassXC with `keepassxc-cli import`. Every item becomes an entry in one group
named `sefy`:

| sefy | KeePass |
| --- | --- |
| title | `Title` |
| `login`, `password`, `url`, `notes` fields | `UserName`, `Password`, `URL`, `Notes` |
| a note's text | `Notes` |
| a one-time password key | `TimeOtp-Secret-Base32` when bare, `otp` when an `otpauth://` link |
| any other field | a string of that name, protected when secret |
| a stored file | an attachment |
| tags | tags |
| identity, times | the entry's UUID and times |
| the kind | the entry's custom data, as `sefy-kind` |

KeePass keeps custom data without showing it, so an entry that comes back
through [`import`](/sefy/reference/import/) is the kind it left as - a card,
not a login with odd fields. A value XML cannot carry at all - most control
characters - stops the export and names the item, rather than being dropped.

### CSV

The columns Chrome writes, which every browser and password manager imports,
and one more for a one-time password key:

```csv
name,url,username,password,note,totp
mail,https://mail.example.com,someone,…,backup codes in the drawer,JBSWY3DPEHPK3PXP
```

A CSV has a row per login and a column per field it knows. Anything else is
left out and counted, never written somewhere it would be misread:

```console
$ sefy export --format csv --i-know-this-writes-plaintext -o logins.csv
wrote logins.csv in the clear: 12 items
left out: 1 note, 1 card - CSV holds logins only; --format keepass carries every kind
1 login has fields a CSV has no column for; those fields are left out
```

## History

An export holds each item's **current** contents. Its
[history](/sefy/reference/history/) - the earlier versions an edit or a merge
kept - stays in the vault: an export is a snapshot to move or keep elsewhere,
and a file of every password an account has ever had, in the clear, is a larger
exposure than that calls for.

When the point is to leave with everything, `--with-history` adds the earlier
versions: as `history` in sefy's JSON, as each entry's history in KeePass XML.
A CSV has no place for them, and the flag is refused there:

```console
$ sefy export --format keepass --with-history --i-know-this-writes-plaintext -o all.xml
wrote all.xml in the clear: 14 items and 31 earlier versions
```

## Related

- [`import`](/sefy/reference/import/) - reading a file back in, from sefy or elsewhere
- [Moving between password managers](/sefy/guides/password-managers/) - arriving and leaving
- [Moving a vault between machines](/sefy/guides/moving-a-vault/)
- [Threat model](/sefy/concepts/threat-model/) - the one deliberate exception
- [Versions and compatibility](/sefy/concepts/versions/) - items written by a newer sefy
