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

## Already here, left alone

Each entry carries the identity its item had in the vault it came from. An
entry whose identity is already here is **skipped**, so importing the same
export twice does not double anything:

```console
$ sefy import backup.json
imported 0 items
3 items already here, left alone
```

Skipped means *untouched*, not updated. An export is a snapshot, and it may
easily be older than what is in the vault now — overwriting a password with one
from last month is exactly the kind of quiet damage worth refusing. To bring
newer contents across, use [`merge`](/sefy/reference/merge/), which compares
both sides and says so when it cannot decide.

Entries **without** an identity are always added. Exports written by sefy 0.1.x
carry none, and neither does JSON written by hand — there is nothing to
recognise them by, and matching on titles instead would silently collapse two
accounts that happen to share a name.

## All or nothing

The whole file is checked before anything is inserted, so a malformed entry
halfway down cannot leave a half-imported vault behind. Either every item lands
or none does.

## Related

- [`export`](/sefy/reference/export/) — producing the file, and its format
- [`merge`](/sefy/reference/merge/) — folding in another vault, newer contents and all
- [Moving a vault between machines](/sefy/guides/moving-a-vault/) — how copies drift
