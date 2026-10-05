# ADR-0006: Exchange formats - what comes in, what goes out, and history with them

- **Status:** accepted
- **Date:** 2026-10-05

## Context

Up to 0.12, `import` read sefy's own JSON and nothing else, and `export` wrote
it. That kept a vault from being a trap only for someone moving between two
copies of sefy. Someone arriving from KeePass, Bitwarden or a browser had no
way in, and someone leaving had a file no other program reads.

0.13.0 reads KeePass XML, Bitwarden JSON and CSVs of passwords, and writes
KeePass XML and CSV beside sefy's JSON. This records how each maps onto sefy,
and what happens to history on the way.

## Decision

### The file says what it is; an export is told what to be

`import` takes no format flag. sefy's JSON names itself (`sefy_export`),
Bitwarden's carries `items` beside `encrypted`, `folders` or `collections`,
KeePass's has a `KeePassFile` root, and anything else is read as a CSV that
must prove itself with a header naming a password, a login or an address. A
flag would be a second way to say what the file already says, and a way to say
it wrongly.

`export` takes `--format sefy|keepass|csv`, defaulting to sefy's JSON, the one
format that loses nothing. There is no Bitwarden writer: Bitwarden imports
KeePass XML and CSV.

### Every format is read into the same shape first

Each reader produces entries - a `NewItem`, the identity, times and earlier
contents the source knew - and notices about what did not come across as it
was. One routine puts entries in the vault. That is what keeps the guarantees
the same for every format: the whole file is read and checked before anything
is inserted, an identity already present is skipped and never overwritten, and
every entry that did not arrive as it was is named in the report.

### Identity comes from the source where it has one

A KeePass entry's UUID, a Bitwarden item's id and Firefox's `guid` become the
item's identity, so importing the same file twice adds nothing the second
time. A source without one - Chrome's CSV - is always added, as JSON without a
`uuid` always was: titles are not identities, and matching on them would
silently collapse two accounts that share a name.

An attachment of a KeePass entry becomes a file item whose identity is derived
from the entry's and the file name, so it is recognised on a second import too.

### A CSV is read by column names, not by which program wrote it

One vocabulary - `name`/`title`, `url`/`login_uri`/`website`,
`username`/`login_username`, `password`, `note`/`notes`/`extra`,
`totp`/`otpauth`, `folder`/`grouping` - reads Chrome, Edge, Firefox, Safari and
the CSVs of most password managers without knowing any of them by name. A
column outside it becomes a secret field of the record: dropping what the file
holds is worse than an untidy record, and for a value of unknown meaning,
hiding it is the mistake that can be undone. Columns known to be a program's
own bookkeeping - `httpRealm`, `formActionOrigin`, `timeLastUsed`, favourite
and reprompt flags - are left out and named in the report.

### Nothing is dropped in silence; what has no kind changes shape

A Bitwarden identity, driver's license or passport has no sefy kind; it becomes
a note listing every field it carried. A note's custom fields are written into
its text. A KeePass attachment becomes a file item. A one-time password key
sefy cannot read - a Steam key - is kept as a secret field named `otp`. Each
of these is a notice of its own in the report.

Only what cannot move at all is left behind: entries in a trash or recycle bin,
a passkey, a Bitwarden linked field (it holds no value, only a pointer), an
entry with nothing in it but a title. Each is named.

### KeePass: one mapping for every kind, and the kind in custom data

A record's `login`, `password`, `url` and `notes` take KeePass's standard
`UserName`, `Password`, `URL` and `Notes`; every other field becomes a string
of its own, protected in memory when it is secret. That is the same for a
login, a card and a Wi-Fi network - no per-kind table to drift.

The sefy kind is written to the entry's custom data under `sefy-kind`. KeePass
and KeePassXC keep custom data without showing it, so a card that goes to
KeePass and comes back is a card, not a login with odd fields. An entry
without it - one KeePass made - is a note when notes are all it holds, and a
login otherwise.

Groups become a tag holding their path without the root (`Internet/Shops`);
KeePass's own tags stay tags. On the way out every entry goes into one group,
with sefy's tags as KeePass tags: an item has many tags and an entry one group,
so building groups from tags would have to drop all but one.

A one-time password key is written the way each program keeps one natively: a
bare key as KeePass 2's `TimeOtp-Secret-Base32`, an `otpauth://` link as
KeePassXC's `otp`, which keeps its issuer and account. Both are read, along with
KeePass's hex, base64 and plain-text secret variants and its algorithm, length
and period. A round trip gives back exactly what was stored.

A value XML 1.0 cannot carry - most control characters - stops the export with
the item's title rather than being dropped. A carriage return is written as
`&#13;`, because XML turns a literal `\r\n` into `\n` on reading. A file whose
values are still sealed with a database's inner key (`Protected="True"`) is
refused whole, with the way to export it properly.

Attachments that KeePass compressed are inflated with `flate2`; XML is parsed
with `quick-xml` and CSV with `csv`. Each covers its format whole - escaping,
quoting, multi-line cells - which is exactly the part a hand-written reader
gets wrong.

### History comes in; it goes out only when asked

Earlier contents the source kept arrive as the item's history:

- a KeePass entry's history snapshots become versions, dated by their
  modification time;
- Bitwarden's `passwordHistory` keeps old values, not old states. Walking back
  from the current contents and putting each old value back gives the state
  before every change; each is dated by the change that ended the one before
  it, the oldest by the item's creation. An entry written as `name: value`
  where `name` is one of the item's hidden fields is that field's old value -
  Bitwarden keeps hidden fields' history that way - and not the password's.

A snapshot identical to the one before it is no version (KeePass keeps one for
a renamed title), and the first of such a run keeps its time, which is when
those contents were written. Versions from another tool have no machine.

The import writes into the encrypted vault, so bringing history in exposes
nothing; refusing it would leave the owner's earlier passwords behind in the
program they are leaving.

On the way out, the 0.12 decision stands: an export holds each item's current
contents. `--with-history` adds the earlier versions for the formats that have
a place for them - sefy's JSON and KeePass XML, as the entry's history. A CSV
has none; in the library that combination cannot be expressed
(`Target::Csv` has no flag), and the command line refuses it with a reason.

### sefy's JSON carries the version its contents are

Every entry now carries `created_at`, `updated_at` and `version` - the uuid,
`seq`, time and machine of the current contents - and, with `--with-history`,
`history`. Without `version`, an import gave the contents a fresh identity; the
vault it came from, edited afterwards, then merged in as a conflict instead of
an update, because neither side recognised the other's version. The format
version stays 1: every new key is optional on the way in and ignored by an
older reader.

## Consequences

- Leaving for KeePass or KeePassXC loses nothing but the machine names of
  versions. Leaving for a browser takes logins only, and the export says what
  it left out.
- An import is never all-or-nothing about *kinds*: an entry sefy cannot store
  is a notice, not a failure. It is all-or-nothing about *validity*: a file
  that is not what it claims to be adds nothing.
- The CSV vocabulary will need a word now and then for a program that names a
  column its own way. Until it has one, the column arrives as a secret field,
  not as nothing.
