---
title: "show"
description: Show an item without revealing its secret fields.
---

Prints an item's surroundings — id, title, kind, tags, and the fields that are
not secret.

## Usage

```sh
sefy show <REFERENCE>
```

For a credential, the password and TOTP secret stay covered, and the output says
where to get them:

```console
$ sefy show mail
id:       2
title:    mail
kind:     credential
tags:     mail
login:    someone@example.com
password: <hidden — use sefy get>
url:      https://mail.example.com
totp:     <hidden — use sefy get --field totp>
notes:    recovery in the drawer
```

For a file, the stored name and size:

```console
$ sefy show id_ed25519
id:       3
title:    id_ed25519
kind:     file
tags:     keys
file:     id_ed25519
size:     387 bytes
```

## Notes print in full

A note has no secret *field* — the body is the item — so `show` prints it after
a rule:

```console
$ sefy show bank
id:       1
title:    bank
kind:     note
tags:     home, money
---
code 4815
```

That means `show` on a note **does** put its contents in your terminal
scrollback. If a note holds something you would rather copy than display, reach
for [`get`](/sefy/reference/get/) instead, which puts it on the clipboard.

## Related

- [`get`](/sefy/reference/get/) — the only way a covered secret comes out
- [`ls`](/sefy/reference/ls/) — the same items, one line each
