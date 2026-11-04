---
title: "agent"
description: Keep the vault unlocked for a while, so that commands stop asking for the password.
---

Asks for the master password once and leaves a process holding it, in memory
only. Until it locks, commands get what they need from it and do not ask.

## Usage

```console
$ sefy agent
Master password:
unlocked for 15m of nobody using it, 12h at most (agent pid 18792)
`sefy lock` locks it now
$ sefy get mail
copied password of "mail" to the clipboard; clearing in 45s
```

| Option | Meaning |
| --- | --- |
| `--ttl <TIME>` | How long the vault stays unlocked with nobody using it: `90s`, `15m`, `2h`. Default `15m`. |

Run it again and it says so and asks for nothing, so it can go in a shell's
profile:

```console
$ sefy agent
already unlocked by the agent, locks in 14m
```

## When it locks

- **With nobody using it**, after `--ttl`. Reading or writing the vault starts
  the time over; [`status`](/sefy/reference/status/) does not, so a prompt that
  shows the state does not keep the vault unlocked.
- **After 12 hours**, however busy it has been, counted from the password.
- **At [`sefy lock`](/sefy/reference/lock/)**, at once.
- **When the vault's password changes or the vault is wiped**:
  [`change-password`](/sefy/reference/change-password/) and
  [`wipe`](/sefy/reference/wipe/) lock it themselves.

Locking is quiet. The process wipes the password and exits, with nobody asking
it to and whether or not anyone is using it; the next command asks for the
password as usual. A machine that sleeps through the time is locked when it
wakes: the time is counted on the wall clock as well.

To go on not being asked, run `sefy agent` again.

## What goes through it

The agent listens on a local socket: a Unix domain socket in a directory only
you can enter, or a named pipe only you can open.

- **The password never crosses it.** It reaches the agent from `sefy agent`
  through an anonymous pipe. A command that finds the vault unlocked never sees
  the password.
- **Keys do**, one version of the file at a time. A vault's key is the password
  stretched with the salt at the front of the file, and
  [every write has a new salt](/sefy/concepts/vault-format/), so a command asks
  for the key for the file as it is and, to save, for a key for a new salt. Each
  file written through the agent is one the password alone opens.
- **The contents do not.** The command opens the file itself.

A command sends nothing but a salt, the first bytes of a file it already has,
and checks what comes back against the file: a wrong key fails the
authentication tag, and then the command asks for the password as if nothing
were running. So there is nothing to learn by posing as an agent, or to gain.

## One password at a time

The agent holds one password. A vault under another one goes on asking, and its
status says why:

```console
$ sefy --vault other.bak status
vault    /home/you/other.bak
size     84.1 KB
lock     locked (what is running holds another vault's password)
```

`--password-env` is always the password used, agent or no agent, so a script
does the same thing on any machine. [`change-password`](/sefy/reference/change-password/)
and [`wipe`](/sefy/reference/wipe/) always ask: they take knowing the password,
not having found the vault unlocked. So does [`merge`](/sefy/reference/merge/),
for the other file.

If a vault locks while a command is running, say over a long edit, the command
asks for the password when it comes to save, up to three times; with no terminal
it says so and writes nothing. A password that is not the one the vault was
opened with is refused rather than used, so a typo cannot move the vault to it.

## What it does not protect from

The socket keeps other users out. It cannot tell one of your programs from
another: anything running as you can ask the agent for a key, and so can read
the vault while it is unlocked. On Linux the agent makes itself undumpable, as
ssh-agent does, so that no other program of yours can read its memory and it
leaves no core file; Windows and macOS offer a program nothing like it that its
owner cannot undo.

The password is in memory for as long as the agent runs, where the system may
page it to disk like anything else. And the pipe or the socket is visible while
it runs: it says that sefy is running, not which file it holds. See the
[threat model](/sefy/concepts/threat-model/).

## The desktop app does the same

While a vault is unlocked in the [desktop app](/sefy/guides/desktop-app/#the-command-line-while-it-is-unlocked)
the command line does not ask for its password either, with the same 15 minutes
and 12 hours. With both running the window has its own place on the socket, so
`sefy lock` reaches both.

## Updating

The installers run `sefy lock` before they replace the program. On Windows a
running program cannot be replaced, so before updating through npm or cargo,
run `sefy lock` yourself.

## Environment

| Variable | Meaning |
| --- | --- |
| `SEFY_AGENT_SOCKET` | Listen and connect here instead: the start of a pipe's name on Windows, of a socket's path elsewhere. For keeping several agents apart. On Unix its directory must be one only you can enter. |

## Related

- [`lock`](/sefy/reference/lock/) - lock it now
- [`status`](/sefy/reference/status/) - whether it is locked, and for how much longer it will not be
- [`doctor`](/sefy/reference/doctor/) - who holds the password on this machine
- [Threat model](/sefy/concepts/threat-model/)
