//! A transport that keeps a sefy vault in a git repository.
//!
//! It is an ordinary executable following the sefy plugin protocol: run with
//! `--manifest` it says what it is, run with `run` it reads a request as JSON
//! on stdin and moves one file.
//!
//! What it moves is the **sealed** vault — a headerless blob. This program
//! never sees the master password and could not read what it carries if it
//! wanted to, which is the whole point of the protocol.
//!
//! # Configuration
//!
//! `SEFY_GITHUB_REPO` — the repository to use, in any form `git clone`
//! accepts. Everything else follows from it: the working copy lives in this
//! program's own data directory, and authentication is whatever git already
//! does on this machine, so no token is stored here.

mod git;
mod repository;

use std::io::Read;
use std::process::ExitCode;

/// What this program answers to `--manifest`.
///
/// The version comes from the manifest rather than being written out again:
/// one number in two places is a release-day trap, and a plugin declaring the
/// wrong version is diagnosed by whoever is trying to work out why a transport
/// misbehaves.
fn manifest() -> String {
    format!(
        r#"{{"protocol_version":1,"name":"github","version":"{}","description":"Keeps a vault in a git repository","operations":["push","pull"]}}"#,
        env!("CARGO_PKG_VERSION")
    )
}

fn main() -> ExitCode {
    let argument = std::env::args().nth(1);

    match argument.as_deref() {
        Some("--manifest") => {
            println!("{}", manifest());
            ExitCode::SUCCESS
        }
        Some("run") => run(),
        Some("--version") => {
            println!("{}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!(
                "sefy-plugin-github is a transport for sefy, not a command to run by hand.\n\
                 \n\
                 Install it where sefy looks for plugins and use `sefy push`, `sefy pull`\n\
                 or `sefy sync`. Set SEFY_GITHUB_REPO to the repository to keep the vault in."
            );
            ExitCode::FAILURE
        }
    }
}

/// Carries out one operation, reporting failure the way the protocol expects.
fn run() -> ExitCode {
    let mut request = String::new();
    if let Err(error) = std::io::stdin().read_to_string(&mut request) {
        return fail(&format!("could not read the request: {error}"));
    }

    match repository::handle(&request) {
        Ok(message) => {
            println!("{}", report(&message));
            ExitCode::SUCCESS
        }
        Err(error) => fail(&error.to_string()),
    }
}

/// Reports a failure on stderr and exits non-zero.
///
/// Both are honoured by sefy; stderr is used because a message there survives
/// even if this program is run by something that ignores the report.
fn fail(reason: &str) -> ExitCode {
    eprintln!("{reason}");
    ExitCode::FAILURE
}

/// A report as one line of JSON.
///
/// Hand-built rather than through serde: the shape is two optional strings, and
/// a dependency for it would be larger than the code it replaces.
fn report(message: &str) -> String {
    format!("{{\"message\":\"{}\"}}", escape(message))
}

/// Escapes a string for a JSON value.
fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            control if control.is_control() => {
                escaped.push_str(&format!("\\u{:04x}", control as u32));
            }
            other => escaped.push(other),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_manifest_is_the_json_the_protocol_asks_for() {
        // Parsed rather than compared as text: what matters is that sefy can
        // read it, and a hand-written constant is exactly the kind of thing
        // that acquires a stray comma.
        let parsed: serde_json::Value = serde_json::from_str(&manifest()).unwrap();

        assert_eq!(parsed["protocol_version"], 1);
        assert_eq!(parsed["name"], "github");
        assert_eq!(parsed["operations"][0], "push");
        assert_eq!(parsed["operations"][1], "pull");
    }

    #[test]
    fn the_manifest_declares_a_version_at_all() {
        // The number itself comes from the package manifest, so the thing worth
        // asserting is that it arrives — a build that lost it would ship a
        // plugin whose version reads as an empty string in `plugin list`.
        let parsed: serde_json::Value = serde_json::from_str(&manifest()).unwrap();

        let version = parsed["version"].as_str().expect("a version must be there");
        assert!(!version.is_empty());
        assert!(version.contains('.'), "got: {version}");
    }

    #[test]
    fn a_report_survives_a_message_with_quotes_in_it() {
        let line = report(r#"pushed "vault" to origin"#);

        let parsed: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(parsed["message"], r#"pushed "vault" to origin"#);
    }

    #[test]
    fn a_report_survives_a_message_with_a_windows_path_in_it() {
        let line = report(r"cloned into C:\Users\someone\AppData");

        let parsed: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(parsed["message"], r"cloned into C:\Users\someone\AppData");
    }
}
