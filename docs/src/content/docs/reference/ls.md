---
title: "ls"
description: List items in the vault.
---

Lists items, newest first: id, title, kind, tags.

## Usage

```sh
sefy ls [OPTIONS]
```

| Option | Meaning |
| --- | --- |
| `--kind <KIND>` | `note`, `credential` or `file`. |
| `--tag <TAG>` | Keep only items carrying **every** listed tag. |

```console
$ sefy ls
3  f.bin  file        [misc]
2  mail   credential  [mail]
1  bank   note        [home, money]
```

```sh
sefy ls --kind credential
sefy ls --tag money,home     # items carrying both, not either
```

Tags are combined with **and**, not or: `--tag money,home` is a narrowing
filter, which is what a listing is for. To see items matching any of several
tags, list them separately.

Nothing secret appears here — only what [`show`](/sefy/reference/show/) would
print at the top. An empty result is not an error:

```console
$ sefy ls --tag nothing
no items
```

## Related

- [`find`](/sefy/reference/find/) — the same listing, narrowed by text
- [`tags`](/sefy/reference/tags/) — which tags exist and how many items use them
