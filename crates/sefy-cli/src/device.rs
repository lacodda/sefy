//! The name of the machine sefy runs on, for the versions it writes.
//!
//! A merge conflict is two machines disagreeing, and the first thing worth
//! knowing about the version that lost is where it came from. The name goes
//! inside the sealed vault only — never beside it, never into a file name.

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

/// `gethostname(2)`: every Unix has it, and the C library it lives in is
/// linked already. A crate for one call would be one more thing a secret
/// store's users have to trust.
#[cfg(unix)]
fn host_name() -> Option<String> {
    use std::ffi::{c_char, c_int};

    unsafe extern "C" {
        fn gethostname(name: *mut c_char, len: usize) -> c_int;
    }

    let mut buffer = [0u8; 256];
    // SAFETY: the pointer and length describe `buffer`, which outlives the
    // call; the result is read only up to the first NUL inside it.
    let status = unsafe { gethostname(buffer.as_mut_ptr().cast(), buffer.len()) };
    if status != 0 {
        return None;
    }
    let end = buffer.iter().position(|&byte| byte == 0)?;
    String::from_utf8(buffer[..end].to_vec()).ok()
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
