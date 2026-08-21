//! Transports that move a vault file between this machine and somewhere else.
//!
//! A plugin is an ordinary executable named `sefy-plugin-*`. sefy asks it what
//! it is (`--manifest`), then asks it to move a file (`run`, with the request
//! as JSON on stdin). Nothing here starts a service; a plugin lives for the
//! duration of one call.
//!
//! # What a plugin never sees
//!
//! A transport is handed a **path to the sealed file** and nothing else. It
//! never receives the master password, a derived key, or a single item's
//! contents — the vault's plaintext exists only inside sefy's own memory, and
//! handing it to a third-party binary would give that away. What a plugin
//! carries is exactly what an onlooker would find on disk: a headerless blob.
//!
//! Because the blob is opaque to the transport, a plugin cannot merge. It
//! fetches the other copy to a path sefy chose, and sefy folds the two together
//! itself with [`crate::merge`], where both sides can actually be read.
//!
//! # The protocol
//!
//! ```text
//! $ sefy-plugin-demo --manifest
//! {"protocol_version":1,"name":"demo","version":"0.1.0","operations":["push","pull"]}
//!
//! $ echo '{"operation":"push","file":"/tmp/blob","name":"vault"}' | sefy-plugin-demo run
//! {"message":"uploaded 4.2 KiB"}
//! ```

pub mod manifest;

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub use manifest::{Manifest, Operation, PREFIX, PROTOCOL_VERSION, Plugin};

/// Where sefy looks for plugins, in order.
///
/// The application's own data directory first, then `PATH`. Deliberately
/// nothing beside the vault file: a `plugins/` directory sitting next to an
/// otherwise anonymous blob would announce what that blob is, which is the one
/// thing the file format spends all its effort avoiding.
pub fn search_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    if let Some(directory) = plugin_directory() {
        paths.push(directory);
    }

    if let Some(system) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&system));
    }

    paths
}

/// The per-user directory sefy keeps plugins in, if the OS offers one.
///
/// Resolved from the environment rather than through a crate: this is the only
/// path sefy needs, and a dependency for it would cost more than it saves.
pub fn plugin_directory() -> Option<PathBuf> {
    data_directory().map(|base| base.join("sefy").join("plugins"))
}

/// Base directory for per-user application data.
fn data_directory() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    }

    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME")
            .filter(|value| !value.is_empty())
            .map(|home| {
                PathBuf::from(home)
                    .join("Library")
                    .join("Application Support")
            })
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        // XDG says a relative XDG_DATA_HOME must be ignored, not resolved
        // against the working directory.
        let from_xdg = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute());

        from_xdg.or_else(|| {
            std::env::var_os("HOME")
                .filter(|value| !value.is_empty())
                .map(|home| PathBuf::from(home).join(".local").join("share"))
        })
    }
}

/// Everything installed, usable or not.
///
/// A plugin that fails to describe itself is still listed, with the reason.
/// Silence would leave the user comparing a missing line against a spelling
/// mistake with no way to tell which they are looking at.
pub fn discover() -> Vec<Plugin> {
    discover_in(&search_paths())
}

/// Everything found in these directories, in order; the first copy of a name
/// wins, as with any other command.
pub fn discover_in(directories: &[PathBuf]) -> Vec<Plugin> {
    let mut found: Vec<Plugin> = Vec::new();

    for directory in directories {
        let Ok(entries) = std::fs::read_dir(directory) else {
            continue;
        };

        for entry in entries.filter_map(std::result::Result::ok) {
            let path = entry.path();
            let Some(name) = executable_name(&path) else {
                continue;
            };
            if !name.starts_with(PREFIX) || name.len() == PREFIX.len() {
                continue;
            }
            if found.iter().any(|plugin| plugin.executable == name) {
                continue;
            }

            found.push(describe(&path, &name));
        }
    }

    found.sort_by(|a, b| a.executable.cmp(&b.executable));
    found
}

/// Finds one plugin among those installed, by short name or executable name.
///
/// Takes the list rather than discovering it: a caller that has to say
/// something useful when the name matches nothing needs the same list to say it
/// with, and discovering twice would run every plugin on the machine again.
pub fn find(installed: Vec<Plugin>, name: &str) -> Option<Plugin> {
    installed.into_iter().find(|plugin| plugin.answers_to(name))
}

/// Asks a plugin what it is.
fn describe(path: &Path, name: &str) -> Plugin {
    let base = Plugin {
        executable: name.to_owned(),
        path: path.to_path_buf(),
        manifest: None,
        usable: false,
        reason: None,
    };

    let output = match command(path).arg("--manifest").output() {
        Ok(output) => output,
        Err(error) => {
            return Plugin {
                reason: Some(format!("could not run it: {error}")),
                ..base
            };
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Plugin {
            reason: Some(if stderr.trim().is_empty() {
                "it refused to describe itself".into()
            } else {
                stderr.trim().to_owned()
            }),
            ..base
        };
    }

    let manifest: Manifest = match serde_json::from_slice(&output.stdout) {
        Ok(manifest) => manifest,
        Err(error) => {
            return Plugin {
                reason: Some(format!("its manifest could not be read: {error}")),
                ..base
            };
        }
    };

    if manifest.protocol_version != PROTOCOL_VERSION {
        return Plugin {
            reason: Some(format!(
                "it speaks protocol {} and this build speaks {PROTOCOL_VERSION}",
                manifest.protocol_version
            )),
            manifest: Some(manifest),
            ..base
        };
    }

    if manifest.operations.is_empty() {
        return Plugin {
            reason: Some("it declares no operations".into()),
            manifest: Some(manifest),
            ..base
        };
    }

    Plugin {
        manifest: Some(manifest),
        usable: true,
        ..base
    }
}

/// What a plugin is told when an operation runs.
///
/// Note what is absent: no password, no key, no item. `file` points at the
/// sealed blob — to push, read it; to pull, write it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request<'a> {
    /// What to do.
    pub operation: Operation,
    /// Local path of the sealed vault file to read (push) or write (pull).
    pub file: &'a Path,
    /// What the remote copy should be called.
    ///
    /// A transport needs *some* handle for the thing it stores, and the local
    /// file name is a poor one — a vault is deliberately named like anything
    /// else, and on a shared remote two machines' "notes.bak" would collide.
    pub name: &'a str,
}

/// What a plugin may send back.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Report {
    /// A line to show the user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Set by a plugin reporting failure without a non-zero exit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Runs one operation of one plugin.
///
/// The request goes in over stdin as JSON and the report comes back over
/// stdout, so a plugin can be written in any language.
pub fn invoke(plugin: &Plugin, request: &Request<'_>) -> Result<Report> {
    if !plugin.usable {
        return Err(Error::PluginUnusable {
            name: plugin.name().to_owned(),
            reason: plugin
                .reason
                .clone()
                .unwrap_or_else(|| "it is not usable".into()),
        });
    }

    if !plugin.supports(request.operation) {
        return Err(Error::PluginUnusable {
            name: plugin.name().to_owned(),
            reason: format!("it does not do {}", request.operation),
        });
    }

    run(&plugin.path, plugin.name(), request)
}

/// Spawns the plugin and reads its verdict.
fn run(path: &Path, name: &str, request: &Request<'_>) -> Result<Report> {
    let payload = serde_json::to_vec(request).map_err(|error| Error::PluginFailed {
        name: name.to_owned(),
        reason: format!("could not build the request: {error}"),
    })?;

    let mut child = command(path)
        .arg("run")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| Error::PluginFailed {
            name: name.to_owned(),
            reason: format!("could not run it: {error}"),
        })?;

    {
        let mut stdin = child.stdin.take().ok_or_else(|| Error::PluginFailed {
            name: name.to_owned(),
            reason: "could not write to it".into(),
        })?;
        if let Err(error) = stdin.write_all(&payload) {
            // A plugin is allowed to answer without reading its input; if it
            // exits first, the write sees a closed pipe. The verdict still
            // arrives through stdout and the exit status, so only a failure
            // other than the closed pipe is real.
            if error.kind() != std::io::ErrorKind::BrokenPipe {
                return Err(Error::PluginFailed {
                    name: name.to_owned(),
                    reason: format!("could not send the request: {error}"),
                });
            }
        }
    }

    let output = child
        .wait_with_output()
        .map_err(|error| Error::PluginFailed {
            name: name.to_owned(),
            reason: format!("it did not finish: {error}"),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::PluginFailed {
            name: name.to_owned(),
            reason: if stderr.trim().is_empty() {
                format!("it failed with {}", output.status)
            } else {
                stderr.trim().to_owned()
            },
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.trim().is_empty() {
        // Silence is success with nothing to say.
        return Ok(Report::default());
    }

    let report: Report = serde_json::from_str(stdout.trim()).map_err(|error| {
        // The plugin's own output is not quoted back: a transport may well
        // print a URL with a token in it, and this message can end up in a log.
        Error::PluginFailed {
            name: name.to_owned(),
            reason: format!("its reply could not be read: {error}"),
        }
    })?;

    if let Some(message) = report.error {
        return Err(Error::PluginFailed {
            name: name.to_owned(),
            reason: message,
        });
    }

    Ok(report)
}

/// Name a plugin is known by: the file name without its extension.
fn executable_name(path: &Path) -> Option<String> {
    if !path.is_file() {
        return None;
    }

    let name = path.file_stem()?.to_str()?;

    // On Windows only these are runnable; without the check, a stray
    // `sefy-plugin-notes.txt` would be treated as a program.
    #[cfg(windows)]
    {
        let runnable = path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                matches!(
                    extension.to_ascii_lowercase().as_str(),
                    "exe" | "cmd" | "bat"
                )
            });
        if !runnable {
            return None;
        }
    }

    Some(name.to_owned())
}

fn command(path: &Path) -> Command {
    // Only the Windows branch below mutates it.
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut command = Command::new(path);

    // Otherwise a console window flashes on every call on Windows.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    command
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes a runnable stand-in for a plugin into `directory`.
    ///
    /// A batch file on Windows, a shell script elsewhere: the point of the
    /// convention is that a plugin is any executable, not a Rust artefact.
    fn fake_plugin(directory: &Path, name: &str, manifest: &str, reply: &str) -> PathBuf {
        std::fs::create_dir_all(directory).unwrap();

        #[cfg(windows)]
        {
            // The payloads are written beside the script and printed from
            // there: cmd's `echo` mangles quotes, and this is about testing
            // sefy, not batch escaping.
            let manifest_path = directory.join(format!("{name}.manifest.json"));
            std::fs::write(&manifest_path, manifest).unwrap();
            let reply_path = directory.join(format!("{name}.reply.json"));
            std::fs::write(&reply_path, reply).unwrap();

            let path = directory.join(format!("{name}.cmd"));
            std::fs::write(
                &path,
                format!(
                    "@echo off\r\nif \"%1\"==\"--manifest\" (type \"{}\") else (type \"{}\")\r\n",
                    manifest_path.display(),
                    reply_path.display()
                ),
            )
            .unwrap();
            path
        }

        #[cfg(not(windows))]
        {
            use std::os::unix::fs::PermissionsExt;
            let path = directory.join(name);
            std::fs::write(
                &path,
                format!(
                    "#!/bin/sh\nif [ \"$1\" = \"--manifest\" ]; then\n  cat <<'MANIFEST'\n{manifest}\nMANIFEST\nelse\n  cat <<'REPLY'\n{reply}\nREPLY\nfi\n"
                ),
            )
            .unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            path
        }
    }

    const BOTH_WAYS: &str =
        r#"{"protocol_version":1,"name":"demo","version":"0.1.0","operations":["push","pull"]}"#;

    fn demo(directory: &Path) -> Plugin {
        fake_plugin(
            directory,
            "sefy-plugin-demo",
            BOTH_WAYS,
            r#"{"message":"moved"}"#,
        );
        discover_in(&[directory.to_path_buf()])
            .into_iter()
            .find(|plugin| plugin.executable == "sefy-plugin-demo")
            .expect("the plugin must be discovered")
    }

    fn request<'a>(file: &'a Path, operation: Operation) -> Request<'a> {
        Request {
            operation,
            file,
            name: "vault",
        }
    }

    #[test]
    fn the_application_directory_comes_before_the_path() {
        // Only meaningful where the OS offers one; every supported platform
        // does, but the resolution goes through the environment.
        if let Some(directory) = plugin_directory() {
            assert_eq!(search_paths().first(), Some(&directory));
        }
    }

    #[test]
    fn plugins_are_never_looked_for_beside_the_vault() {
        // The whole point of the format is that the vault file gives nothing
        // away; a directory beside it would.
        let paths = search_paths();

        assert!(!paths.iter().any(|path| path == Path::new("")));
        assert!(!paths.iter().any(|path| path == Path::new(".")));
    }

    #[test]
    fn an_empty_directory_contributes_no_plugins() {
        let directory = tempfile::tempdir().unwrap();

        assert!(discover_in(&[directory.path().to_path_buf()]).is_empty());
    }

    #[test]
    fn a_well_formed_plugin_is_discovered_and_usable() {
        let directory = tempfile::tempdir().unwrap();

        let plugin = demo(directory.path());

        assert!(plugin.usable, "reason: {:?}", plugin.reason);
        assert_eq!(plugin.name(), "demo");
        assert!(plugin.supports(Operation::Push));
        assert!(plugin.supports(Operation::Pull));
    }

    #[test]
    fn a_plugin_speaking_another_protocol_is_listed_but_refused() {
        let directory = tempfile::tempdir().unwrap();
        fake_plugin(
            directory.path(),
            "sefy-plugin-future",
            r#"{"protocol_version":99,"name":"future","version":"9.0.0","operations":["push"]}"#,
            "{}",
        );

        let plugin = discover_in(&[directory.path().to_path_buf()])
            .into_iter()
            .find(|plugin| plugin.executable == "sefy-plugin-future")
            .expect("it must still be listed");

        assert!(!plugin.usable);
        assert!(
            plugin.reason.as_deref().unwrap_or_default().contains("99"),
            "the mismatch must be shown, not hidden"
        );
    }

    #[test]
    fn a_plugin_with_an_unreadable_manifest_says_why() {
        let directory = tempfile::tempdir().unwrap();
        fake_plugin(
            directory.path(),
            "sefy-plugin-broken",
            "this is not json",
            "{}",
        );

        let plugin = discover_in(&[directory.path().to_path_buf()])
            .into_iter()
            .find(|plugin| plugin.executable == "sefy-plugin-broken")
            .expect("a broken plugin must still be listed");

        assert!(!plugin.usable);
        assert!(plugin.reason.is_some());
        assert_eq!(plugin.name(), "broken", "it must still be addressable");
    }

    #[test]
    fn a_plugin_declaring_no_operations_is_refused() {
        let directory = tempfile::tempdir().unwrap();
        fake_plugin(
            directory.path(),
            "sefy-plugin-idle",
            r#"{"protocol_version":1,"name":"idle","version":"0.1.0"}"#,
            "{}",
        );

        let plugin = discover_in(&[directory.path().to_path_buf()])
            .into_iter()
            .find(|plugin| plugin.executable == "sefy-plugin-idle")
            .expect("it must still be listed");

        assert!(!plugin.usable);
    }

    #[test]
    fn a_file_without_the_prefix_is_not_a_plugin() {
        let directory = tempfile::tempdir().unwrap();
        fake_plugin(directory.path(), "something-else", BOTH_WAYS, "{}");

        assert!(discover_in(&[directory.path().to_path_buf()]).is_empty());
    }

    #[test]
    fn the_bare_prefix_is_not_a_plugin() {
        let directory = tempfile::tempdir().unwrap();
        fake_plugin(directory.path(), "sefy-plugin-", BOTH_WAYS, "{}");

        assert!(discover_in(&[directory.path().to_path_buf()]).is_empty());
    }

    #[test]
    fn the_first_directory_wins_when_a_name_appears_twice() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        fake_plugin(first.path(), "sefy-plugin-demo", BOTH_WAYS, "{}");
        fake_plugin(
            second.path(),
            "sefy-plugin-demo",
            r#"{"protocol_version":1,"name":"other","version":"9.9.9","operations":["push"]}"#,
            "{}",
        );

        let found = discover_in(&[first.path().to_path_buf(), second.path().to_path_buf()]);

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name(), "demo");
    }

    #[test]
    fn an_operation_runs_and_its_report_comes_back() {
        let directory = tempfile::tempdir().unwrap();
        let plugin = demo(directory.path());
        let file = directory.path().join("blob");

        let report = invoke(&plugin, &request(&file, Operation::Push)).unwrap();

        assert_eq!(report.message.as_deref(), Some("moved"));
    }

    #[test]
    fn a_request_carries_the_file_and_nothing_secret() {
        let file = Path::new("/tmp/blob");

        let json = serde_json::to_value(request(file, Operation::Pull)).unwrap();

        assert_eq!(json["operation"], "pull");
        assert_eq!(json["name"], "vault");
        let mut keys: Vec<&str> = json
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec!["file", "name", "operation"],
            "a transport is given the sealed file and nothing else"
        );
    }

    #[test]
    fn an_operation_the_plugin_does_not_declare_is_refused_before_running() {
        let directory = tempfile::tempdir().unwrap();
        fake_plugin(
            directory.path(),
            "sefy-plugin-oneway",
            r#"{"protocol_version":1,"name":"oneway","version":"0.1.0","operations":["push"]}"#,
            r#"{"message":"moved"}"#,
        );
        let plugin = discover_in(&[directory.path().to_path_buf()])
            .into_iter()
            .next()
            .unwrap();
        let file = directory.path().join("blob");

        let error = invoke(&plugin, &request(&file, Operation::Pull)).unwrap_err();

        assert!(error.to_string().contains("pull"), "got: {error}");
    }

    #[test]
    fn an_unusable_plugin_is_never_run() {
        let directory = tempfile::tempdir().unwrap();
        // It declares the operation being asked for, so only the usability
        // check can stop it. A plugin with no readable manifest would be
        // refused by the `supports` check instead, and this test would pass
        // without the barrier it is named after ever running.
        fake_plugin(
            directory.path(),
            "sefy-plugin-future",
            r#"{"protocol_version":99,"name":"future","version":"9.0.0","operations":["push"]}"#,
            r#"{"message":"should never run"}"#,
        );
        let plugin = discover_in(&[directory.path().to_path_buf()])
            .into_iter()
            .next()
            .unwrap();
        assert!(
            plugin
                .manifest
                .as_ref()
                .is_some_and(|manifest| manifest.operations.contains(&Operation::Push)),
            "the fixture must reach the usability check, not stop before it"
        );
        let file = directory.path().join("blob");

        let error = invoke(&plugin, &request(&file, Operation::Push)).unwrap_err();

        assert!(!error.to_string().contains("should never run"));
        assert!(error.to_string().contains("99"), "got: {error}");
    }

    #[test]
    fn a_plugin_reporting_an_error_in_its_reply_fails_the_call() {
        let directory = tempfile::tempdir().unwrap();
        fake_plugin(
            directory.path(),
            "sefy-plugin-sad",
            BOTH_WAYS,
            r#"{"error":"no credentials for the remote"}"#,
        );
        let plugin = discover_in(&[directory.path().to_path_buf()])
            .into_iter()
            .next()
            .unwrap();
        let file = directory.path().join("blob");

        let error = invoke(&plugin, &request(&file, Operation::Push)).unwrap_err();

        assert!(error.to_string().contains("no credentials"), "got: {error}");
    }

    #[test]
    fn silence_is_success_with_nothing_to_say() {
        let directory = tempfile::tempdir().unwrap();
        fake_plugin(directory.path(), "sefy-plugin-quiet", BOTH_WAYS, "");
        let plugin = discover_in(&[directory.path().to_path_buf()])
            .into_iter()
            .next()
            .unwrap();
        let file = directory.path().join("blob");

        let report = invoke(&plugin, &request(&file, Operation::Push)).unwrap();

        assert!(report.message.is_none());
    }

    #[test]
    fn a_plugin_is_found_by_its_short_name() {
        let directory = tempfile::tempdir().unwrap();
        let plugin = demo(directory.path());
        let installed = vec![plugin];

        let found = find(installed, "demo").expect("the short name must address it");

        assert_eq!(found.executable, "sefy-plugin-demo");
    }

    #[test]
    fn a_plugin_is_also_found_by_its_executable_name() {
        let directory = tempfile::tempdir().unwrap();
        let plugin = demo(directory.path());

        let found = find(vec![plugin], "sefy-plugin-demo");

        assert!(found.is_some(), "the file name must address it too");
    }

    #[test]
    fn a_name_matching_nothing_finds_nothing() {
        let directory = tempfile::tempdir().unwrap();
        let plugin = demo(directory.path());

        assert!(find(vec![plugin], "elsewhere").is_none());
    }

    #[test]
    fn a_plugin_too_broken_to_describe_itself_is_still_addressable() {
        // The case that matters: naming it is how someone asks what is wrong
        // with it, and a plugin with no manifest has no short name of its own.
        let directory = tempfile::tempdir().unwrap();
        fake_plugin(directory.path(), "sefy-plugin-broken", "not json", "{}");
        let installed = discover_in(&[directory.path().to_path_buf()]);

        let found = find(installed, "broken").expect("it must still answer to its name");

        assert!(!found.usable);
    }
    #[test]
    fn an_unreadable_reply_is_not_quoted_back() {
        let directory = tempfile::tempdir().unwrap();
        // A transport printing a signed URL is exactly the case that must not
        // end up in a message someone pastes into an issue.
        fake_plugin(
            directory.path(),
            "sefy-plugin-chatty",
            BOTH_WAYS,
            "https://remote.example/upload?token=SECRETVALUE",
        );
        let plugin = discover_in(&[directory.path().to_path_buf()])
            .into_iter()
            .next()
            .unwrap();
        let file = directory.path().join("blob");

        let error = invoke(&plugin, &request(&file, Operation::Push)).unwrap_err();

        assert!(!error.to_string().contains("SECRETVALUE"), "got: {error}");
    }
}
