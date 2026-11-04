//! Getting to an open vault: where the file is, and what the password is.

use anyhow::{Context, Result, anyhow, bail};
use sefy_core::agent::{Client, LockState, Seat};
use sefy_core::crypto::{DerivedKey, SALT_LEN, random_bytes};
use sefy_core::{Error, FreshKey, KeySource, Opener, Vault};
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use zeroize::Zeroizing;

/// Resolves which vault file to work on.
///
/// The path comes from `--vault` or `$SEFY_VAULT`, and there is no default. A
/// conventional location like `~/.sefy/vault` would undo the point of a file
/// that looks like nothing in particular.
pub fn vault_path(argument: Option<PathBuf>) -> Result<PathBuf> {
    argument.ok_or_else(|| {
        anyhow!(
            "no vault given\n\
             pass --vault <FILE>, or set SEFY_VAULT to the path of your vault.\n\
             sefy has no default location on purpose: a predictable path would\n\
             give the file away."
        )
    })
}

/// What this command opened the vault with, and so what opens anything else
/// it reaches: a remote copy, the copies beside the file.
#[derive(Clone)]
pub enum Master {
    /// The password, typed here or read from the environment.
    Typed(Zeroizing<String>),
    /// An agent's word for it; this process never saw the password.
    Held(Arc<HeldKeys>),
}

impl Master {
    /// What opens another sealed file under the same password.
    pub fn opener(&self) -> Opener<'_> {
        match self {
            Self::Typed(password) => Opener::Password(password.as_bytes()),
            Self::Held(keys) => Opener::Source(keys.as_ref()),
        }
    }
}

/// Keys from an agent, for one command.
///
/// A command can outlast the agent it started with: someone takes twenty
/// minutes over a note in an editor, and by the time it is saved the vault has
/// locked. What they wrote should not be lost to that, so from then on the
/// answer comes from the person, once. It is checked against the key that
/// opened the vault as it is typed, so a typo asks again rather than throwing
/// away the twenty minutes; the vault checks it once more before sealing.
pub struct HeldKeys {
    client: Client,
    /// Whether the vault has been opened: before that, an agent that cannot
    /// help is an agent to pass over, not a reason to ask.
    opened: AtomicBool,
    /// The salt and key that opened it, to tell the right password by.
    opened_with: Mutex<Option<([u8; SALT_LEN], DerivedKey)>>,
    typed: Mutex<Option<Zeroizing<String>>>,
}

/// How many times a command that outlasted its agent asks for the password.
const ATTEMPTS: usize = 3;

impl HeldKeys {
    fn new(client: Client) -> Self {
        Self {
            client,
            opened: AtomicBool::new(false),
            opened_with: Mutex::new(None),
            typed: Mutex::new(None),
        }
    }

    /// The password, asked for the first time it is needed.
    fn typed(&self) -> sefy_core::Result<Zeroizing<String>> {
        let mut typed = self.typed.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(password) = typed.as_ref() {
            return Ok(password.clone());
        }

        let opened_with = self
            .opened_with
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let (salt, key) = opened_with.as_ref().ok_or(Error::KeysGone)?;
        let mut question = "The vault locked while this was running. Master password: ";
        for _ in 0..ATTEMPTS {
            let answer = Zeroizing::new(prompt(question).map_err(|_| Error::KeysGone)?);
            if DerivedKey::derive(answer.as_bytes(), salt)?.same_as(key) {
                *typed = Some(answer.clone());
                return Ok(answer);
            }
            question = "That is not it. Master password: ";
        }
        Err(Error::KeysChanged)
    }

    fn may_ask(&self) -> bool {
        self.opened.load(Ordering::SeqCst)
    }

    /// The seat this asks, and what is there now.
    fn holder(&self) -> Option<(Seat, std::time::Duration)> {
        match self.client.state() {
            Some(LockState::Unlocked { left, .. }) => Some((self.client.seat(), left)),
            _ => None,
        }
    }
}

impl KeySource for HeldKeys {
    fn key_for(&self, salt: &[u8; SALT_LEN]) -> sefy_core::Result<DerivedKey> {
        let key = match self.client.key_for(salt) {
            Err(Error::KeysGone) if self.may_ask() => {
                DerivedKey::derive(self.typed()?.as_bytes(), salt)
            }
            other => other,
        }?;
        if !self.may_ask() {
            // Asked for on the way to opening the vault. If this is not the
            // key that opens it, it is replaced or dropped with the rest.
            *self
                .opened_with
                .lock()
                .unwrap_or_else(PoisonError::into_inner) = Some((*salt, key.duplicate()));
        }
        Ok(key)
    }

    fn fresh(&self, opened_with: &[u8; SALT_LEN]) -> sefy_core::Result<FreshKey> {
        match self.client.fresh(opened_with) {
            Err(Error::KeysGone) if self.may_ask() => {
                let password = self.typed()?;
                let salt = random_bytes::<SALT_LEN>()?;
                Ok(FreshKey {
                    salt,
                    key: DerivedKey::derive(password.as_bytes(), &salt)?,
                    proof: DerivedKey::derive(password.as_bytes(), opened_with)?,
                })
            }
            other => other,
        }
    }
}

/// Opens the vault for a command: through an agent if one can, or with the
/// password.
///
/// `--password-env` comes first when it is given. Whoever passes a password
/// has said which one they mean, and a script that passes one should do the
/// same thing on a machine with an agent running as on one without.
pub fn unlock(path: &Path, password_env: Option<&str>) -> Result<(Vault, Master)> {
    if password_env.is_none()
        && let Some(opened) = through_agent(path)?
    {
        return Ok(opened);
    }
    typed(path, password_env)
}

/// Opens the vault with the password and nothing else: for what takes knowing
/// it, rather than having found the vault unlocked.
pub fn typed(path: &Path, password_env: Option<&str>) -> Result<(Vault, Master)> {
    let password = password(password_env)?;
    let vault = open(path, &password)?;
    Ok((vault, Master::Typed(Zeroizing::new(password))))
}

/// Opens the vault through whichever agent can, if any can.
///
/// An agent holding another vault's password gives a key that does not fit,
/// which is an agent to pass over: it is no more a wrong password than a key
/// from one that has locked is. Anything else is about the file, and is said.
pub fn through_agent(path: &Path) -> Result<Option<(Vault, Master)>> {
    open_through_agent(path, false)
}

/// The same, for looking at the vault rather than using it: an agent's idle
/// time does not start over, so a prompt showing the state of the vault does
/// not keep it unlocked.
pub fn looking_through_agent(path: &Path) -> Result<Option<(Vault, Master)>> {
    open_through_agent(path, true)
}

fn open_through_agent(path: &Path, looking: bool) -> Result<Option<(Vault, Master)>> {
    for seat in Seat::ALL {
        let Some(client) = Client::of(seat) else {
            continue;
        };
        let client = if looking {
            client.only_looking()
        } else {
            client
        };
        let keys = Arc::new(HeldKeys::new(client));
        match Vault::open_through(path, keys.clone()) {
            Ok(mut vault) => {
                vault.set_device(crate::device::name());
                keys.opened.store(true, Ordering::SeqCst);
                return Ok(Some((vault, Master::Held(keys))));
            }
            Err(Error::KeysGone | Error::WrongPasswordOrNotAVault | Error::TooSmall) => {}
            Err(other) => return Err(other.into()),
        }
    }
    Ok(None)
}

/// Tells every agent to wipe what it holds, and says which ones held anything.
pub fn lock_everywhere() -> Vec<&'static str> {
    Seat::ALL
        .into_iter()
        .filter(|seat| Client::of(*seat).and_then(|client| client.lock()) == Some(true))
        .map(Seat::name)
        .collect()
}

/// Who holds a password right now, wherever they sit, and for how much longer.
pub fn holders() -> Vec<(Seat, std::time::Duration)> {
    Seat::ALL
        .into_iter()
        .filter_map(|seat| {
            let client = Client::of(seat)?;
            match client.state()? {
                LockState::Unlocked { left, .. } => Some((seat, left)),
                LockState::Locked => None,
            }
        })
        .collect()
}

/// `unlocked by the agent, locks in 14m`, or what stands in its place.
pub fn lock_line(path: &Path, master: Option<&Master>) -> Result<String> {
    // Asked again rather than assumed from how this command opened the file:
    // `--password-env` opens it with an agent running all the same.
    let held = match master {
        Some(Master::Held(keys)) => keys.holder(),
        _ => match looking_through_agent(path)? {
            Some((_, Master::Held(keys))) => keys.holder(),
            _ => None,
        },
    };
    Ok(match held {
        Some((seat, left)) => format!(
            "unlocked by the {}, locks in {}",
            seat.name(),
            crate::when::span(left)
        ),
        None if holders().is_empty() => "locked".to_owned(),
        // Worth saying: it is the one case where `sefy agent` having been run
        // does not mean this vault stopped asking.
        None => "locked (what is running holds another vault's password)".to_owned(),
    })
}

/// Reads the master password of an existing vault.
pub fn password(password_env: Option<&str>) -> Result<String> {
    if let Some(variable) = password_env {
        return from_env(variable);
    }
    prompt("Master password: ")
}

/// Reads a new master password, asking twice so a typo cannot lock the vault.
pub fn new_password(password_env: Option<&str>) -> Result<String> {
    if let Some(variable) = password_env {
        return from_env(variable);
    }

    let first = prompt("New master password: ")?;
    let second = prompt("Repeat it: ")?;
    if first != second {
        bail!("the two passwords do not match");
    }
    Ok(first)
}

/// Reads a secret that belongs *inside* the vault, such as an account password.
pub fn secret(prompt_text: &str, password_env: Option<&str>) -> Result<String> {
    if let Some(variable) = password_env {
        return from_env(variable);
    }
    prompt(prompt_text)
}

fn from_env(variable: &str) -> Result<String> {
    std::env::var(variable).with_context(|| format!("cannot read the password from ${variable}"))
}

/// Asks for a password without echoing it.
///
/// The prompt talks to the terminal itself (`/dev/tty`, `CONIN$`), not to
/// stdin, so a command whose input is piped can still ask: `sefy run` hands
/// its stdin to the command it starts, and `add note` reads the note from it.
/// What it refuses is asking when no stream is a terminal — a script or a
/// service, where nobody is there to answer and a prompt would be a hang.
fn prompt(text: &str) -> Result<String> {
    let someone_is_there = std::io::stdin().is_terminal()
        || std::io::stdout().is_terminal()
        || std::io::stderr().is_terminal();
    if !someone_is_there {
        bail!(
            "cannot ask for a password: sefy is not attached to a terminal\n\
             use --password-env <VAR> to pass it through the environment,\n\
             or run `sefy agent` first to keep the vault unlocked for a while."
        );
    }
    rpassword::prompt_password(text).context("cannot read the password")
}

/// Opens an existing vault, turning a wrong password into a clear message.
///
/// The vault is told which machine it is on, so every version written through
/// it says so.
pub fn open(path: &Path, password: &str) -> Result<Vault> {
    match Vault::open(path, password.as_bytes()) {
        Ok(mut vault) => {
            vault.set_device(crate::device::name());
            Ok(vault)
        }
        Err(Error::WrongPasswordOrNotAVault) => bail!(
            "wrong password, or {} is not a vault\n\
             (an encrypted file cannot tell the two apart)",
            path.display()
        ),
        Err(other) => Err(other.into()),
    }
}
