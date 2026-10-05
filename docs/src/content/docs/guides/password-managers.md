---
title: Moving between password managers
description: Bring your passwords in from KeePass, Bitwarden or a browser, and take them out again - with their history, and with a list of what did not fit.
---

Arriving from another password manager is one export there and one
[`import`](/sefy/reference/import/) here. Leaving is one
[`export`](/sefy/reference/export/) here and one import there. This guide is
the walk through both, and about the file in between, which holds every
password you have in the clear.

## Before you start

Every file below is unencrypted. Make it in a folder that is not synced or
backed up, import it, look at what arrived, and delete it. Where both ends can
use a pipe, use one, and the file never touches the disk at all.

Start with a fresh vault rather than one you already use, so a mistake costs a
`rm` and nothing else:

```sh
export SEFY_VAULT=~/vaults/new.bak
sefy init
```

## From Bitwarden

In the web vault or an app: **Tools → Export vault**, format **`.json`** - not
`.json (Encrypted)`, which only Bitwarden can read. With the command-line
client:

```sh
bw export --format json --output ./bitwarden_export.json
sefy import ./bitwarden_export.json
```

Folders become tags. The last few passwords Bitwarden kept for each login come
along as that item's [history](/sefy/reference/history/), so `sefy history
github` shows when the password changed and `sefy restore` can bring one back.

Two things do not come across, and the import names each item they belong to:
passkeys, which no password manager can export, and items in the trash. An
identity, a driver's license or a passport becomes a note listing its fields,
because sefy has no kind for them.

## From KeePass or KeePassXC

KeePass 2: **File → Export → KeePass XML (2.x)**. KeePassXC:

```sh
keepassxc-cli export --format xml ./passwords.kdbx > ./keepass.xml
sefy import ./keepass.xml
```

Each group becomes a tag with its path - an entry in `Internet/Shops` is
tagged `Internet/Shops` - and the recycle bin is left behind. One-time password
keys come across whether KeePass or KeePassXC set them up. Attachments become
file items of their own, titled after the entry: an SSH key attached to
`server` arrives as `server - id_ed25519`. An entry's history becomes the
item's.

KeePassXC's XML export leaves the attachments' bytes out, so from KeePassXC
they have to travel separately. The import lists each one; save it and add it
as a file:

```sh
keepassxc-cli attachment-export ./passwords.kdbx server id_ed25519 ./id_ed25519
sefy add file ./id_ed25519
```

## From a browser

| Browser | Where |
| --- | --- |
| Chrome, Edge, Brave, Opera, Vivaldi | Settings → Passwords (Password Manager) → Export passwords |
| Firefox | `about:logins` → ⋯ → Export Logins |
| Safari | File → Export → Passwords |

```sh
sefy import ./Chrome\ Passwords.csv
```

Every row becomes a login, named after its site when the browser gave it no
name. Firefox's file carries an identity for each login, so importing it twice
adds nothing the second time; Chrome's does not, so importing a Chrome file
twice gives you every login twice.

A CSV from another password manager is read the same way: columns are
recognised by name, a folder column becomes a tag, and a column sefy does not
recognise is kept as a secret field instead of being dropped.

## Check what arrived

The import ends with a list of everything that did not come across exactly as
it was - left behind, brought in part, or brought in another shape - each with
its title and why. Read it before deleting the file you imported from:

```sh
sefy ls
sefy show github
sefy history github
```

## Leaving sefy

To KeePass or KeePassXC - the format that keeps every kind, every field and,
when asked, every earlier version:

```sh
sefy export --format keepass --with-history \
  --i-know-this-writes-plaintext -o ./sefy.xml
keepassxc-cli import ./sefy.xml ./passwords.kdbx
```

In KeePass 2, **File → Import → KeePass XML (2.x)**. Bitwarden reads the same
file under "KeePass 2 (xml)".

Leave `--with-history` out unless you mean to take every earlier password
along: without it the file holds what is current and nothing more.

To a browser, logins only:

```sh
sefy export --format csv --i-know-this-writes-plaintext -o ./logins.csv
```

Then the browser's "Import passwords". The export says on stderr what a CSV had
no place for - notes, cards, files, a login's extra fields - so you know what
to carry some other way.

## Related

- [`import`](/sefy/reference/import/) - every format, and how entries map
- [`export`](/sefy/reference/export/) - the three formats and what each holds
- [Threat model](/sefy/concepts/threat-model/) - why the file in between is the weak point
