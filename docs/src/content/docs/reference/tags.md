---
title: "tags"
description: List the tags in use.
---

Lists the tags in use, with the number of items carrying each.

## Usage

```console
$ sefy tags
home   1
mail   1
misc   1
money  1
```

## Tags are not managed separately

There is no command to create or delete a tag. A tag exists because an item
carries it, and disappears when the last item stops:

```console
$ sefy edit "bank card" --tag money    # was: home, money
updated 1

$ sefy tags
mail   1
misc   1
money  1
```

`home` is gone without being deleted — nothing carried it any more. This keeps
the tag list honest by construction: it can never show a tag that would match no
items.

## Related

- [`ls`](/sefy/reference/ls/) — filter by tag with `--tag`
- [`edit`](/sefy/reference/edit/) — change an item's tags
