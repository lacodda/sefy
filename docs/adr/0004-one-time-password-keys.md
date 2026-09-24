# ADR-0004: One-time password keys, and what a template says about filling

- **Status:** accepted
- **Date:** 2026-09-24

## Context

A login has carried a `totp` field from the start, but sefy never did anything
with it: the value was stored and handed back, and the code a site asks for
came from some other app. 0.10.0 makes the code itself, lets a key be stored at
the moment a site shows it, draws it as a QR code for a phone, and hands a
login's fields over one at a time for a sign-in form.

Four questions follow: what to compute codes with, what to store, what to
accept, and where the order of filling lives.

## Decision

### Codes are made by sefy-core, on RustCrypto HMAC

RFC 6238 is HMAC over a time counter plus a truncation rule; it is some forty
lines. It is written in `sefy_core::otp` on `hmac`, `sha1` and `sha2` - the same
audited RustCrypto family as the vault's Argon2id and XChaCha20-Poly1305 - and
checked against the RFC's appendix B vectors for all three hashes.

A dedicated TOTP crate was not taken. It would bring its own URI parser and its
own opinion on key length (the common one refuses keys under 128 bits, which
real sites still issue at 80), and the part that matters - the MAC - would
still be RustCrypto underneath.

The core owns it, not the CLI, so the GUI planned for a later stage makes codes
the same way.

### A key is stored as it came

The field is still `totp`, secret. Its value is one of two shapes:

- an `otpauth://totp/` link, kept **word for word**. Its issuer, account,
  digits, period and algorithm are the site's own, codes are made with exactly
  those, and a QR code drawn from it shows the phone what the site meant it to
  show;
- a bare base32 key, stored **without** the spaces, dashes and padding setup
  pages print it with, and upper-cased, so one key is always one string. It
  means the defaults every site assumes: SHA-1, six digits, thirty seconds.

Converting every key into a link was considered and rejected: a link needs a
label, the only label available for a bare key is the record's title, and a
title changes while a stored label would not.

The field's name is what makes it a key, in any record - the same way `url` is
what `sefy open` opens. There is no per-field "format" flag to disagree with
the name.

### A key is checked on the way in, everywhere

`add login --totp`, `edit --set totp=`, `edit --set-secret totp`, `otp --set`
and the template-driven `add` all pass the value through `otp::normalize`. A key
that cannot make a code is found out while the setup page is still open, not at
the next sign-in when the key is gone. Counter-based (HOTP) links are refused
by name.

Keys already in a vault are not rewritten: a vault from before 0.10.0 may hold
a `totp` value in any shape, and `sefy otp` reports a value it cannot use and
says how to replace it rather than failing to open the record.

### A template says which fields a form wants

`FieldSpec` gains `fill`. `sefy fill` walks the template's fields in order,
takes those marked and present, and turns a `totp` field into a code at the
moment its turn comes. A login fills login, password, code; a card fills
number, holder, expiry and CVV but not its PIN, which is typed at a terminal
rather than into a form.

The order lives in the template rather than in the command so that a new kind
of record brings its own, and there is one place to read it.

## Consequences

- `sefy-core` gains the `otp` module, `Totp`, `Error::InvalidOtpKey`, the
  `fill` field on `FieldSpec` and `Template::fill_order`. Code that builds a
  `FieldSpec` or matches `Error` exhaustively has to follow.
- The CLI gains `qrcodegen` (no dependencies of its own) and takes `console`
  directly - it was already in the tree through `dialoguer` - to switch a
  Windows console into reading the colour escapes a QR code is drawn with.
- The vault schema does not change.
