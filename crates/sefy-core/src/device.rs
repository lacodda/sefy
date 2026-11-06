//! The name of the machine sefy runs on, for the versions it writes.
//!
//! A merge conflict is two machines disagreeing, and the first thing worth
//! knowing about the version that lost is where it came from. The name goes
//! inside the sealed vault only: never beside it, never into a file name.
//!
//! Here rather than in a program around the core because there are two of them
//! writing versions now, and a vault should not say `laptop` for what the command
//! line wrote there and nothing for what the window did.

/// This machine's name, or nothing when it cannot be found.
///
/// The host name up to its first dot: `laptop`, not `laptop.local` or a
/// domain someone else chose. A missing name is not an error; the versions
/// written here simply say nothing about where.
pub fn name() -> Option<String> {
    let raw = host_name()?;
    let short = raw.split('.').next().unwrap_or_default().trim();
    (!short.is_empty()).then(|| short.to_owned())
}

/// On Windows the system sets it for every process.
#[cfg(windows)]
fn host_name() -> Option<String> {
    std::env::var("COMPUTERNAME").ok()
}

/// `uname(2)`, through the crate that already keeps this library's other Unix
/// calls safe: this one forbids `unsafe` of its own.
#[cfg(unix)]
fn host_name() -> Option<String> {
    Some(
        rustix::system::uname()
            .nodename()
            .to_string_lossy()
            .into_owned(),
    )
}

#[cfg(not(any(windows, unix)))]
fn host_name() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_name_found_is_short_and_never_blank() {
        // What the machine is called is not known here; what shape the answer
        // has is.
        if let Some(name) = super::name() {
            assert!(!name.is_empty());
            assert!(!name.contains('.'), "{name}");
            assert_eq!(name.trim(), name);
        }
    }
}
