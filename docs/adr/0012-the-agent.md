# ADR-0012: The agent

- **Status:** accepted
- **Date:** 2026-11-03

## Context

Every command that touches a vault asks for the master password, and stretches
it for most of a second. That is right for a command run now and then, and wrong
for ten in a row, or for a script that has nobody to ask.

So something has to keep the password between commands, and that runs into the
line ADR-0001 draws: what is decrypted lives only in sefy's memory. A keyring or
an encrypted session file would be readable by the next process, which means by
that process's neighbours too, and would survive a reboot. The cache lives in the
memory of a process and nowhere else, as ssh-agent's and gpg-agent's do.
ADR-0001 stands unchanged.

What is less obvious is *what* it holds, and it decides most of the design.

## Decision

### It holds the password, and gives out keys

A vault's key is the password stretched with the salt at the front of its file,
and ADR-0001 samples a new salt on every write. So a key is good for one version
of one file: holding a derived key alone would stop working at the first save.
The ways out were:

- **Keep the salt from one write to the next**, so that one key lasts. Then every
  version of the file begins with the same sixteen bytes. Anyone holding two
  versions, which a git remote and a server's snapshots are, sees a stable prefix
  over changing noise: the signature the format exists not to have. Rejected.
- **Hold the key and ask for the password on every write**, and on every pull and
  sync, whose remote copy has a salt of its own. It leaves the agent useful only
  for reading. Rejected.
- **Hold the password, in the agent alone, and hand out keys.** Chosen.

Nothing a client receives outlives the file version it was asked for, except
that a client can always ask again; what it cannot do is learn the password. The
command line, which until now held the password for the life of every command,
no longer sees it at all when the vault is unlocked.

### What crosses the socket

Two questions get keys: the key for this salt, and a key for a salt the agent has
just sampled. Whoever asks does the sealing and opening itself, so the plaintext
database never crosses the socket, and each write through the agent has a salt
of its own and is a file the password alone opens.

The password never crosses it. It reaches `sefy agent` from a terminal or an
environment variable, through an anonymous pipe to the process it starts, and it
reaches the desktop app from its Unlock screen. Nothing on the socket is able to
give an agent a password, so a command that has just asked for one cannot leak it
to something posing as an agent.

The protocol is one length byte and that many bytes, in buffers that wipe
themselves: a key that passed through JSON would leave strings behind. An unknown
message gets an answer saying so, and a client that cannot get what it asked for
falls back on asking for the password, so builds of different ages get along.

### The file authenticates the agent

A client sends only a salt, the public first bytes of a file it already has, and
checks the key it gets against that file's authentication tag. So posing as an
agent yields nothing: not a password, not a path, not a way in. A wrong key means
the same as no agent, and the command asks for the password. No check of the
server's identity is needed, and none is made.

The server side is what needs guarding, since it hands out keys. On Unix the
socket lives in a directory of mode 0700, which an agent refuses to listen in
otherwise; on Windows the pipe's descriptor grants everything to its owner and
nothing to anyone else (`D:P(A;;GA;;;OW)`), where the default would let anyone on
the machine open it for reading.

### Sealing takes proof

The dangerous case is a command that opened the vault through one agent and saves
through another. It takes a long edit: the agent locks, another comes up with
another vault's password, the editor closes. Sealing under what that one gives
would quietly move the vault to a password its owner is not thinking of.

So asking for a fresh key means naming the salt the vault was opened with, and the
one answer carries the key for that salt too. The vault compares it with the key
that opened it, and refuses (`Error::KeysChanged`) unless they are the same bytes.

A vault that locked during a command is the ordinary version of this: nothing
replaced the agent, there is just no agent left, and what someone typed should not
be lost to it. The command asks for the password then, up to three times, and checks
each answer against the key that opened the vault, so a typo asks again rather than
throwing the note away. With no terminal it says so and writes nothing.

### When it locks

**Expiry wipes.** A check made when a request arrives refuses the request and
leaves the password in memory for as long as nobody comes, which is how
gpg-agent's cache behaves. The clock here is a thread that looks every second and
wipes, whether or not anyone is asking.

**The process ends with it.** `sefy agent` exits on locking. "Locked" is the state
of there being no such process, with nothing left waiting to be given a password
again and nothing that can linger in its memory. The next command asks as usual,
and to go on not being asked, `sefy agent` is run again.

**Two limits.** Fifteen minutes of nobody using it, or as `--ttl` says, and twelve
hours from the password however busy it is: otherwise a script asking every few
minutes would keep it for good. Reading or writing counts as use. Looking does
not, so `sefy status` asks for its key in a way that does not start the time over,
and a prompt showing the state does not keep the vault unlocked.

**Both clocks.** `Instant` does not advance while a machine sleeps on Linux and
macOS, so on its own it would wake a laptop after a night with minutes still to
run. The wall clock can be set back. Time passed is whichever says more. A tick
rather than one long sleep, for the same reason.

`change-password` and `wipe` take the password typed, always, and lock everything
afterward: retiring a password takes knowing it, and an emergency wipe should not
leave it in a process that outlives the file.

### `status` never asks

Whether a vault is locked is not a question to answer only for someone holding the
password. Locked, with no password given, `status` prints the file, its size and
`lock locked`, exits zero, and says what would show the rest. It used to ask.

### The window is an agent too

While a vault is unlocked in the desktop app, the command line asks the window.
The window holds the password anyway, in the vault it has open, so this adds no
new exposure, and it brings the window under the same limits. It now locks by
itself, through the one path every lock takes: the vault closed, the clipboard
taken back, quick search destroyed, the page told.

There are two places on the socket, one for each kind of agent. With one, a window
unlocked beside a running `sefy agent` would find the name taken, go unheard, and
stay unlocked through a `sefy lock` that reported success.

The window's idle time counts what a person does (pressing, typing, scrolling) and
not what its page asks for, since a card's one-time code asks every thirty seconds
with nobody in the room.

A window taking a place on the socket has to be one window, so a second launch now
hands its file to the first through `tauri-plugin-single-instance` and exits. A
launch on another file locks the vault that is open: someone asked for another
vault, and the first does not stay unlocked behind it.

### A resident window is not here

Closing the window still exits the program, and quick search goes with it. ADR-0011
expected that to change with this agent. The two turn out to be separate questions:
everything here works with the window open, and a window that stays after closing
needs a tray and an answer to what "quit" means. It stays a later decision.

## Consequences

- The password is in memory for up to twelve hours in one more process, where the
  system may page it like anything else. That is CWE-316, accepted and stated in the
  threat model. `sefy agent` is opt-in; without it nothing changes.
- Anything running as the same user can ask for a key, and so can read the vault
  while it is unlocked. A socket cannot tell one of your programs from another, and
  neither can a keyring: GNOME's own documentation calls hiding one application's
  secrets from another in the same security context theatre. On Linux the process
  makes itself undumpable, as ssh-agent does; elsewhere the system offers nothing
  its owner cannot undo.
- The name of the pipe or the socket says that sefy is running. It does not say
  which file: `sefy agent` is never told the path, and nothing that crosses the
  socket names one.
- A long-lived `sefy.exe` cannot be replaced on Windows, so the installer locks
  first and moves a program still running out of the way.
- `sefy-core` gains the `agent` feature and `interprocess`. It keeps
  `#![forbid(unsafe_code)]`: the unsafe calls a named pipe needs are in that crate.

## Alternatives considered

- **The system keyring** (Credential Manager, Keychain, Secret Service). It survives
  a reboot, can be read offline by the standard tools (MITRE T1555), and would put a
  named entry for sefy in a store everyone knows where to look. Rejected, as the
  plan said it would be.
- **A session key in an environment variable**, as `BW_SESSION` is. It would be
  inherited by every program the shell starts, with no way to expire. Rejected.
- **A wrapped key on disk where hardware holds the wrapping**, as 1Password does with
  the Secure Enclave and the TPM. It is the honest form of "survives a reboot", and
  belongs with the Windows Hello stage rather than here.
- **Telling a locked agent the next password typed**, so that it stays up and needs
  no second start. That sends the password over the socket, to whatever is listening
  there, which would then have to be authenticated. Rejected for a simpler rule: the
  password never crosses it.
