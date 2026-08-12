---
title: "find"
description: Search items by text, kind and tags.
---

The same listing as [`ls`](/sefy/reference/ls/), narrowed by text.

## Usage

```sh
sefy find [TEXT] [OPTIONS]
```

| Option | Meaning |
| --- | --- |
| `--kind <KIND>` | `note`, `credential` or `file`. |
| `--tag <TAG>` | Keep only items carrying **every** listed tag. |

```console
$ sefy find mail
2  mail  credential  [mail]
```

```sh
sefy find bank --kind credential
```

## What is searched

Titles, note bodies and credential fields. The **contents of stored files are
not**: a match inside a binary would say nothing useful, and searching them
would mean decompressing and scanning every attachment on every query.

Finding nothing is a normal result, not an error:

```console
$ sefy find zzz
no items
```

This is where `find` differs from a `<REFERENCE>`: commands that act on one item
refuse to guess between several matches, while `find` exists precisely to show
you all of them.

## Related

- [`ls`](/sefy/reference/ls/) — the unfiltered listing
- [`show`](/sefy/reference/show/) — one item in full
