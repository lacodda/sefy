//! Handing a URL to the desktop's browser.
//!
//! A URL out of a vault is data the user typed months ago, and a field named
//! `url` can hold any text at all. Two separate things therefore have to be
//! true before it is handed anywhere, and they are easy to confuse:
//!
//! - **it has to mean a web page.** `file:///`, `javascript:` and
//!   `ms-settings:` are all things a launcher will act on, and none of them is
//!   what "open the site for this login" means. That is the check below.
//! - **it must not be able to become syntax.** That is not solved by checking
//!   characters — `?a=1&b=2` is an ordinary URL and `&` is ordinary shell
//!   syntax, so a character filter strict enough to be safe would refuse real
//!   addresses. It is solved by never involving a shell: every launcher here
//!   is a program receiving one argument, and on Windows that means
//!   `rundll32 url.dll,FileProtocolHandler` rather than `cmd /c start`, whose
//!   `start` is a `cmd` builtin and therefore parsed by `cmd`.

use anyhow::{Context, Result, bail};
use std::process::Command;

/// Opens `url` in whatever the desktop uses for http links.
///
/// Fails rather than guesses when the value does not look like a web address:
/// the user sees what is stored and can fix it, which beats a browser opening
/// something surprising.
pub fn open(url: &str) -> Result<()> {
    check(url)?;
    launch(url)
}

/// Whether a stored value means a web page.
///
/// `http` and `https` only, with a host after the scheme. Deliberately
/// narrower than the URL standard: this opens the site a login belongs to, and
/// a login does not belong to a local file or a script.
///
/// Control characters are refused separately. They cannot appear in a valid
/// URL, and unlike the shell metacharacters they would be a sign that the
/// field holds something other than an address.
fn check(url: &str) -> Result<()> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .with_context(|| {
            format!("{url:?} is not an http(s) address; sefy only opens web addresses")
        })?;

    if rest.is_empty() {
        bail!("{url:?} has no host");
    }

    if let Some(bad) = rest.chars().find(|c| c.is_control() || c.is_whitespace()) {
        bail!("{url:?} contains {bad:?}, which cannot appear in a web address");
    }

    Ok(())
}

/// Hands a checked URL to the platform's opener.
///
/// `rundll32` invokes the registered protocol handler directly. `cmd /c start`
/// is the better-known incantation and the wrong one here: `start` is a
/// builtin, so the URL would pass through `cmd`'s parser, where `&` — a
/// perfectly ordinary character in a query string — separates commands.
#[cfg(windows)]
fn launch(url: &str) -> Result<()> {
    Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", url])
        .status()
        .context("cannot reach the browser through rundll32")?;
    Ok(())
}

/// Hands a checked URL to the platform's opener.
#[cfg(target_os = "macos")]
fn launch(url: &str) -> Result<()> {
    Command::new("open")
        .arg(url)
        .status()
        .context("cannot run `open` to reach the browser")?;
    Ok(())
}

/// Hands a checked URL to the platform's opener.
#[cfg(all(unix, not(target_os = "macos")))]
fn launch(url: &str) -> Result<()> {
    Command::new("xdg-open")
        .arg(url)
        .status()
        .context("cannot run `xdg-open`; is xdg-utils installed?")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_web_addresses_pass() {
        for url in [
            "https://example.com",
            "http://example.com",
            "https://user.example.com:8443/a/b#fragment",
            // A query string with two parameters: the `&` that a character
            // filter would have had to refuse, and the reason no shell is
            // involved in opening it.
            "https://example.com/search?q=1&page=2",
            "https://example.com/a%20b",
        ] {
            assert!(
                check(url).is_ok(),
                "{url} should be allowed: {:?}",
                check(url)
            );
        }
    }

    #[test]
    fn anything_that_is_not_http_is_refused() {
        // Each is a thing some launcher would act on, and none is a site a
        // login belongs to.
        for url in [
            "file:///C:/Windows/System32/calc.exe",
            "javascript:alert(1)",
            "ms-settings:privacy",
            "ftp://example.com",
            "example.com",
            "",
        ] {
            assert!(check(url).is_err(), "{url:?} should be refused");
        }
    }

    #[test]
    fn a_scheme_with_no_host_is_refused() {
        assert!(check("https://").is_err());
        assert!(check("http://").is_err());
    }

    #[test]
    fn whitespace_and_control_characters_are_refused() {
        // Not shell defence - that is the launcher's shape. These simply
        // cannot be in an address, so a field holding them holds something
        // else and the user should be told rather than have a browser open.
        assert!(check("https://exam ple.com").is_err());
        assert!(check("https://example.com\nstart calc").is_err());
        assert!(check("https://example.com\tx").is_err());
    }
}
