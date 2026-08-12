---
title: "import"
description: Add the contents of an export to this vault.
---

Adds the contents of an [export](/sefy/reference/export/) to this vault,
reading stdin when no path is given.

## Usage

```console
$ sefy import backup.json
imported 1 item
```

```sh
sefy --vault ./old.bak export --i-know-this-writes-plaintext \
  | sefy --vault ./new.bak import
```

## Appended, never merged

Items are **appended**: importing into a vault that already holds them produces
duplicates rather than overwriting anything.

Merging would need an identity for items that the format does not carry — two
credentials called "mail" may well be two different accounts — and silently
replacing someone's secrets is worse than a visible duplicate you can remove
with [`rm`](/sefy/reference/rm/).

For that reason a repeated full import is a poor way to sync two vaults. Trim
the export to the items that are actually new first; the format is plain enough
to edit by hand.

## All or nothing

The whole file is checked before anything is inserted, so a malformed entry
halfway down cannot leave a half-imported vault behind. Either every item lands
or none does.

## Related

- [`export`](/sefy/reference/export/) — producing the file, and its format
- [Moving a vault between machines](/sefy/guides/moving-a-vault/) — merging two copies
