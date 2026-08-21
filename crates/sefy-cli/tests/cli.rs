//! End-to-end tests of the command line: what a user types, what comes back.
//!
//! Passwords travel through the environment rather than a terminal, which is
//! the same path scripts use. The clipboard is never touched — `--stdout` is
//! how these tests read secrets back.

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use std::path::{Path, PathBuf};

const MASTER: &str = "master password";

struct Fixture {
    _directory: tempfile::TempDir,
    path: PathBuf,
}

impl Fixture {
    /// A directory with no vault in it yet.
    fn empty() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("notes.bak");
        Self {
            _directory: directory,
            path,
        }
    }

    /// A directory holding a freshly created vault.
    fn with_vault() -> Self {
        let fixture = Self::empty();
        fixture.sefy().arg("init").assert().success();
        fixture
    }

    /// `sefy` pointed at this vault, with the master password in the
    /// environment.
    fn sefy(&self) -> Command {
        let mut command = Command::cargo_bin("sefy").unwrap();
        command
            .env("SEFY_VAULT", &self.path)
            .env("SEFY_TEST_PASSWORD", MASTER)
            .arg("--password-env")
            .arg("SEFY_TEST_PASSWORD");
        command
    }

    fn directory(&self) -> &Path {
        self.path.parent().unwrap()
    }
}

/// Adds a note and returns nothing; failures surface as test failures.
fn add_note(fixture: &Fixture, title: &str, text: &str, tags: &[&str]) {
    let mut command = fixture.sefy();
    command.args(["add", "note", title, "--text", text]);
    if !tags.is_empty() {
        command.arg("--tag").arg(tags.join(","));
    }
    command.assert().success();
}

#[test]
fn init_creates_a_vault_and_refuses_to_overwrite_one() {
    let fixture = Fixture::empty();

    fixture
        .sefy()
        .arg("init")
        .assert()
        .success()
        .stdout(contains("created"));
    assert!(fixture.path.exists());

    fixture
        .sefy()
        .arg("init")
        .assert()
        .failure()
        .stderr(contains("already exists"));
}

#[test]
fn without_a_vault_path_the_error_explains_the_options() {
    let mut command = Command::cargo_bin("sefy").unwrap();
    command
        .env_remove("SEFY_VAULT")
        .arg("ls")
        .assert()
        .failure()
        .stderr(contains("--vault"))
        .stderr(contains("SEFY_VAULT"));
}

#[test]
fn a_wrong_password_is_reported_without_guessing_why() {
    let fixture = Fixture::with_vault();

    Command::cargo_bin("sefy")
        .unwrap()
        .env("SEFY_VAULT", &fixture.path)
        .env("WRONG", "not the password")
        .args(["--password-env", "WRONG", "ls"])
        .assert()
        .failure()
        .stderr(contains("wrong password"));
}

#[test]
fn a_note_survives_a_round_trip_through_the_file() {
    let fixture = Fixture::with_vault();
    add_note(&fixture, "bank", "vault code 4815", &["money", "home"]);

    fixture
        .sefy()
        .args(["get", "bank", "--stdout"])
        .assert()
        .success()
        .stdout(contains("vault code 4815"));

    fixture
        .sefy()
        .args(["show", "bank"])
        .assert()
        .success()
        .stdout(contains("home, money"));
}

#[test]
fn a_credential_stores_its_password_and_hides_it_in_show() {
    let fixture = Fixture::with_vault();

    fixture
        .sefy()
        .env("ITEM_PASSWORD", "hunter2")
        .args([
            "add",
            "credential",
            "mail",
            "--login",
            "someone",
            "--url",
            "https://example.invalid",
            "--item-password-env",
            "ITEM_PASSWORD",
        ])
        .assert()
        .success();

    fixture
        .sefy()
        .args(["get", "mail", "--stdout"])
        .assert()
        .success()
        .stdout(contains("hunter2"));

    fixture
        .sefy()
        .args(["get", "mail", "--field", "login", "--stdout"])
        .assert()
        .success()
        .stdout(contains("someone"));

    // `show` prints the surroundings of a secret, never the secret.
    fixture
        .sefy()
        .args(["show", "mail"])
        .assert()
        .success()
        .stdout(contains("someone"))
        .stdout(contains("https://example.invalid"))
        .stdout(contains("hunter2").not());
}

#[test]
fn the_item_password_never_falls_back_to_the_master_password() {
    let fixture = Fixture::with_vault();

    // No --item-password-env and no terminal: prompting is impossible, and
    // quietly storing the master password instead would be far worse.
    fixture
        .sefy()
        .args(["add", "credential", "mail", "--login", "someone"])
        .write_stdin("")
        .assert()
        .failure()
        .stderr(contains("not a terminal"));
}

#[test]
fn a_file_comes_back_byte_for_byte() {
    let fixture = Fixture::with_vault();
    let source = fixture.directory().join("keyfile.bin");
    let bytes: Vec<u8> = (0..=255u8).cycle().take(100_000).collect();
    std::fs::write(&source, &bytes).unwrap();

    fixture
        .sefy()
        .args(["add", "file"])
        .arg(&source)
        .assert()
        .success();

    let restored = fixture.directory().join("restored.bin");
    fixture
        .sefy()
        .args(["extract", "keyfile"])
        .arg("-o")
        .arg(&restored)
        .assert()
        .success();

    assert_eq!(std::fs::read(&restored).unwrap(), bytes);

    // A second extract must not silently clobber what is already there.
    fixture
        .sefy()
        .args(["extract", "keyfile"])
        .arg("-o")
        .arg(&restored)
        .assert()
        .failure()
        .stderr(contains("--force"));
}

#[test]
fn getting_a_file_points_at_extract_instead() {
    let fixture = Fixture::with_vault();
    let source = fixture.directory().join("keyfile.bin");
    std::fs::write(&source, b"bytes").unwrap();
    fixture
        .sefy()
        .args(["add", "file"])
        .arg(&source)
        .assert()
        .success();

    fixture
        .sefy()
        .args(["get", "keyfile"])
        .assert()
        .failure()
        .stderr(contains("sefy extract"));
}

#[test]
fn an_ambiguous_reference_lists_the_candidates() {
    let fixture = Fixture::with_vault();
    add_note(&fixture, "mail — personal", "a", &[]);
    add_note(&fixture, "mail — work", "b", &[]);

    fixture
        .sefy()
        .args(["get", "mail"])
        .assert()
        .failure()
        .stderr(contains("2 items match"))
        .stderr(contains("mail — personal"))
        .stderr(contains("mail — work"))
        .stderr(contains("use an id"));
}

#[test]
fn an_exact_title_beats_a_substring() {
    let fixture = Fixture::with_vault();
    add_note(&fixture, "mail", "exact one", &[]);
    add_note(&fixture, "mailing list", "the other", &[]);

    fixture
        .sefy()
        .args(["get", "mail", "--stdout"])
        .assert()
        .success()
        .stdout(contains("exact one"));
}

#[test]
fn a_reference_matching_nothing_says_so() {
    let fixture = Fixture::with_vault();
    add_note(&fixture, "bank", "text", &[]);

    fixture
        .sefy()
        .args(["get", "nowhere"])
        .assert()
        .failure()
        .stderr(contains("nothing matches"));
}

#[test]
fn ls_and_find_narrow_by_kind_and_tag() {
    let fixture = Fixture::with_vault();
    add_note(&fixture, "bank pin", "4815", &["money"]);
    add_note(&fixture, "grocery list", "milk", &["home"]);

    fixture
        .sefy()
        .args(["ls"])
        .assert()
        .success()
        .stdout(contains("bank pin"))
        .stdout(contains("grocery list"));

    fixture
        .sefy()
        .args(["ls", "--tag", "money"])
        .assert()
        .success()
        .stdout(contains("bank pin"))
        .stdout(contains("grocery list").not());

    fixture
        .sefy()
        .args(["find", "milk"])
        .assert()
        .success()
        .stdout(contains("grocery list"))
        .stdout(contains("bank pin").not());

    fixture
        .sefy()
        .args(["find", "bank", "--kind", "credential"])
        .assert()
        .success()
        .stdout(contains("no items"));
}

#[test]
fn edit_changes_what_it_is_asked_to_and_nothing_else() {
    let fixture = Fixture::with_vault();
    add_note(&fixture, "old title", "the text", &["stale"]);

    fixture
        .sefy()
        .args([
            "edit",
            "old title",
            "--title",
            "new title",
            "--tag",
            "fresh",
        ])
        .assert()
        .success();

    fixture
        .sefy()
        .args(["get", "new title", "--stdout"])
        .assert()
        .success()
        .stdout(contains("the text"));

    fixture
        .sefy()
        .args(["tags"])
        .assert()
        .success()
        .stdout(contains("fresh"))
        .stdout(contains("stale").not());
}

#[test]
fn edit_rejects_flags_meant_for_another_kind() {
    let fixture = Fixture::with_vault();
    add_note(&fixture, "a note", "text", &[]);

    fixture
        .sefy()
        .args(["edit", "a note", "--login", "someone"])
        .assert()
        .failure()
        .stderr(contains("note"));

    fixture
        .sefy()
        .args(["edit", "a note"])
        .assert()
        .failure()
        .stderr(contains("nothing to change"));
}

#[test]
fn rm_takes_the_item_out_of_the_file() {
    let fixture = Fixture::with_vault();
    add_note(&fixture, "temporary", "text", &[]);

    fixture
        .sefy()
        .args(["rm", "temporary", "-y"])
        .assert()
        .success()
        .stdout(contains("removed"));

    fixture
        .sefy()
        .args(["ls"])
        .assert()
        .success()
        .stdout(contains("no items"));
}

#[test]
fn rm_without_a_terminal_refuses_rather_than_assuming_yes() {
    let fixture = Fixture::with_vault();
    add_note(&fixture, "precious", "text", &[]);

    fixture
        .sefy()
        .args(["rm", "precious"])
        .write_stdin("")
        .assert()
        .failure()
        .stderr(contains("--yes"));

    // Still there.
    fixture
        .sefy()
        .args(["get", "precious", "--stdout"])
        .assert()
        .success()
        .stdout(contains("text"));
}

#[test]
fn change_password_takes_a_separate_variable_for_the_new_one() {
    let fixture = Fixture::with_vault();
    add_note(&fixture, "kept", "text", &[]);

    fixture
        .sefy()
        .env("NEW_PASSWORD", "a different password")
        .args(["change-password", "--new-password-env", "NEW_PASSWORD"])
        .assert()
        .success();

    // The old password no longer opens it.
    fixture
        .sefy()
        .arg("ls")
        .assert()
        .failure()
        .stderr(contains("wrong password"));

    Command::cargo_bin("sefy")
        .unwrap()
        .env("SEFY_VAULT", &fixture.path)
        .env("NEW_PASSWORD", "a different password")
        .args(["--password-env", "NEW_PASSWORD", "get", "kept", "--stdout"])
        .assert()
        .success()
        .stdout(contains("text"));
}

#[test]
fn secrets_do_not_appear_in_the_file_on_disk() {
    let fixture = Fixture::with_vault();
    add_note(
        &fixture,
        "thing",
        "xyzzy-plugh-secret",
        &["a-distinctive-tag"],
    );

    let bytes = std::fs::read(&fixture.path).unwrap();
    for needle in ["xyzzy-plugh-secret", "a-distinctive-tag", "SQLite format 3"] {
        assert!(
            !bytes
                .windows(needle.len())
                .any(|window| window == needle.as_bytes()),
            "{needle:?} leaked into the vault file"
        );
    }
}

#[test]
fn working_with_a_vault_leaves_no_other_files_behind() {
    let fixture = Fixture::with_vault();
    add_note(&fixture, "one", "first", &[]);
    add_note(&fixture, "two", "second", &[]);
    fixture.sefy().args(["rm", "one", "-y"]).assert().success();

    let entries: Vec<_> = std::fs::read_dir(fixture.directory())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(entries, vec![fixture.path.file_name().unwrap().to_owned()]);
}

#[test]
fn export_refuses_until_the_plaintext_warning_is_acknowledged() {
    let fixture = Fixture::with_vault();
    add_note(&fixture, "bank", "code 4815", &[]);

    fixture
        .sefy()
        .arg("export")
        .assert()
        .failure()
        .stderr(contains("in the clear"))
        .stderr(contains("--i-know-this-writes-plaintext"));

    fixture
        .sefy()
        .args(["export", "--i-know-this-writes-plaintext"])
        .assert()
        .success()
        .stdout(contains("sefy_export"))
        .stdout(contains("code 4815"));
}

#[test]
fn an_export_survives_a_round_trip_through_the_command_line() {
    let source = Fixture::with_vault();
    add_note(&source, "bank", "code 4815", &["money"]);
    source
        .sefy()
        .env("ITEM_PASSWORD", "hunter2")
        .args([
            "add",
            "credential",
            "mail",
            "--login",
            "someone",
            "--item-password-env",
            "ITEM_PASSWORD",
        ])
        .assert()
        .success();

    let dump = source.directory().join("dump.json");
    source
        .sefy()
        .args(["export", "--i-know-this-writes-plaintext", "-o"])
        .arg(&dump)
        .assert()
        .success();

    let destination = Fixture::with_vault();
    destination
        .sefy()
        .arg("import")
        .arg(&dump)
        .assert()
        .success()
        .stdout(contains("imported 2 items"));

    destination
        .sefy()
        .args(["get", "bank", "--stdout"])
        .assert()
        .success()
        .stdout(contains("code 4815"));
    destination
        .sefy()
        .args(["get", "mail", "--stdout"])
        .assert()
        .success()
        .stdout(contains("hunter2"));
}

#[test]
fn export_does_not_overwrite_a_file_without_force() {
    let fixture = Fixture::with_vault();
    add_note(&fixture, "bank", "code", &[]);
    let dump = fixture.directory().join("dump.json");
    std::fs::write(&dump, b"existing").unwrap();

    fixture
        .sefy()
        .args(["export", "--i-know-this-writes-plaintext", "-o"])
        .arg(&dump)
        .assert()
        .failure()
        .stderr(contains("--force"));

    assert_eq!(std::fs::read(&dump).unwrap(), b"existing");
}

#[test]
fn import_reads_stdin_and_reports_malformed_input() {
    let fixture = Fixture::with_vault();

    fixture
        .sefy()
        .arg("import")
        .write_stdin(r#"{"sefy_export":1,"items":[{"title":"x","kind":"note","text":"y"}]}"#)
        .assert()
        .success()
        .stdout(contains("imported 1 item"));

    fixture
        .sefy()
        .arg("import")
        .write_stdin(r#"{"sefy_export":1,"items":[{"title":"x","kind":"note"}]}"#)
        .assert()
        .failure()
        .stderr(contains("malformed"));

    fixture
        .sefy()
        .arg("import")
        .write_stdin("not json")
        .assert()
        .failure()
        .stderr(contains("not a sefy export"));
}

#[test]
fn get_clears_the_clipboard_after_the_timeout() {
    let fixture = Fixture::with_vault();
    add_note(&fixture, "bank", "code 4815", &[]);

    // A one-second timeout keeps the test quick; the message has to name the
    // wait before it happens, not after.
    let assertion = fixture
        .sefy()
        .args(["get", "bank", "--clear-after", "1"])
        .assert();

    // A headless CI runner may have no clipboard at all, which is a legitimate
    // outcome here — what must not happen is a hang or a wrong message.
    let output = assertion.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stdout.contains("clearing in 1s") || stderr.contains("cannot reach the clipboard"),
        "unexpected output:\nstdout: {stdout}\nstderr: {stderr}"
    );
}

#[test]
fn the_editor_is_refused_when_there_is_nowhere_to_open_one() {
    let fixture = Fixture::with_vault();

    // No terminal: an editor would block forever or open an unwanted window.
    fixture
        .sefy()
        .env("EDITOR", "vi")
        .args(["add", "note", "x", "--editor"])
        .write_stdin("")
        .assert()
        .failure()
        .stderr(contains("not a terminal"));

    // And with no editor configured, sefy says so rather than guessing at one.
    fixture
        .sefy()
        .env_remove("EDITOR")
        .env_remove("VISUAL")
        .args(["add", "note", "x", "--editor"])
        .write_stdin("")
        .assert()
        .failure()
        .stderr(contains("no editor configured").or(contains("not a terminal")));
}

#[test]
fn the_editor_flag_is_refused_on_items_that_are_not_notes() {
    let fixture = Fixture::with_vault();
    fixture
        .sefy()
        .env("ITEM_PASSWORD", "hunter2")
        .args([
            "add",
            "credential",
            "mail",
            "--login",
            "someone",
            "--item-password-env",
            "ITEM_PASSWORD",
        ])
        .assert()
        .success();

    fixture
        .sefy()
        .env("EDITOR", "vi")
        .args(["edit", "mail", "--editor"])
        .assert()
        .failure()
        .stderr(contains("credential"));
}

#[test]
fn completions_are_generated_for_every_supported_shell() {
    for shell in ["bash", "zsh", "fish", "powershell", "elvish"] {
        Command::cargo_bin("sefy")
            .unwrap()
            .args(["completions", shell])
            .assert()
            .success()
            .stdout(contains("sefy"));
    }
}

#[test]
fn completions_and_help_need_no_vault_and_no_password() {
    for arguments in [
        vec!["--help"],
        vec!["--version"],
        vec!["completions", "bash"],
        vec!["plugin", "list"],
    ] {
        Command::cargo_bin("sefy")
            .unwrap()
            .env_remove("SEFY_VAULT")
            .args(&arguments)
            .assert()
            .success();
    }
}

/// Writes a runnable stand-in for a plugin, and returns the directory holding
/// it — ready to be put on the PATH of a test invocation.
fn plugin_directory(manifest: &str) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();

    #[cfg(windows)]
    {
        let manifest_path = directory.path().join("manifest.json");
        std::fs::write(&manifest_path, manifest).unwrap();
        std::fs::write(
            directory.path().join("sefy-plugin-demo.cmd"),
            format!(
                "@echo off\r\nif \"%1\"==\"--manifest\" (type \"{}\") else (echo {{}})\r\n",
                manifest_path.display()
            ),
        )
        .unwrap();
    }

    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        let path = directory.path().join("sefy-plugin-demo");
        // `printf` is a shell builtin, so this script needs nothing on PATH —
        // the test below cuts PATH down to this directory alone, and a `cat`
        // here would make the plugin fail to describe itself for a reason that
        // has nothing to do with what is being tested.
        std::fs::write(
            &path,
            format!(
                "#!/bin/sh\nif [ \"$1\" = \"--manifest\" ]; then\n  printf '%s' '{manifest}'\nelse\n  printf '%s' '{{}}'\nfi\n"
            ),
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    directory
}

/// `sefy` that can see this directory's plugins and no others.
///
/// PATH is cut down to the directory itself so a plugin installed on the
/// machine running the tests cannot make an assertion pass or fail, and the
/// per-user data directory is pointed at the same place for the same reason.
fn sefy_seeing_only(directory: &Path) -> Command {
    let mut command = Command::cargo_bin("sefy").unwrap();
    command
        .env_remove("SEFY_VAULT")
        .env("PATH", directory)
        .env("APPDATA", directory)
        .env("XDG_DATA_HOME", directory)
        .env("HOME", directory);
    command
}

#[test]
fn an_installed_plugin_is_listed_with_what_it_can_do() {
    let directory = plugin_directory(
        r#"{"protocol_version":1,"name":"demo","version":"1.2.3","operations":["push","pull"]}"#,
    );

    sefy_seeing_only(directory.path())
        .args(["plugin", "list"])
        .assert()
        .success()
        .stdout(
            contains("demo")
                .and(contains("1.2.3"))
                .and(contains("push")),
        );
}

#[test]
fn a_plugin_speaking_another_protocol_is_listed_with_the_reason() {
    let directory = plugin_directory(
        r#"{"protocol_version":99,"name":"demo","version":"1.2.3","operations":["push"]}"#,
    );

    sefy_seeing_only(directory.path())
        .args(["plugin", "list"])
        .assert()
        .success()
        // Present but refused: a line saying nothing would be indistinguishable
        // from the plugin not being installed at all.
        .stdout(
            contains("demo")
                .and(contains("unusable"))
                .and(contains("99")),
        );
}

#[test]
fn nothing_installed_says_how_a_plugin_is_installed() {
    let directory = tempfile::tempdir().unwrap();

    sefy_seeing_only(directory.path())
        .args(["plugin", "list"])
        .assert()
        .success()
        .stdout(contains("no plugins installed").and(contains("sefy-plugin-")));
}

/// Writes a transport whose "remote" is another file on this machine, into a
/// directory that can be put on a test invocation's PATH.
///
/// It reads the path to move from the request on stdin — the same barrier the
/// core tests rely on, and the reason a fixture told the path some other way
/// would prove nothing.
fn transport_directory(remote: &Path) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    let manifest =
        r#"{"protocol_version":1,"name":"file","version":"0.1.0","operations":["push","pull"]}"#;
    let remote = remote.display().to_string();
    // Whatever `name` arrives in the request is recorded here, so a test can
    // check what sefy told the transport rather than only what it printed.
    let seen = directory.path().join("seen-name.txt").display().to_string();

    #[cfg(windows)]
    {
        let manifest_path = directory.path().join("manifest.json");
        std::fs::write(&manifest_path, manifest).unwrap();

        let script = directory.path().join("transport.ps1");
        std::fs::write(
            &script,
            format!(
                "$request = [Console]::In.ReadToEnd() | ConvertFrom-Json\r\n\
                 Set-Content -LiteralPath '{seen}' -Value $request.name -NoNewline\r\n\
                 if ($request.operation -eq 'push') {{ Copy-Item -LiteralPath $request.file -Destination '{remote}' -Force }}\r\n\
                 else {{ Copy-Item -LiteralPath '{remote}' -Destination $request.file -Force }}\r\n"
            ),
        )
        .unwrap();

        // PowerShell by absolute path: these tests cut PATH down to the
        // transport directory, and a bare `powershell` would fail to start for
        // a reason that has nothing to do with what is under test.
        let shell = PathBuf::from(std::env::var_os("SystemRoot").unwrap())
            .join("System32")
            .join("WindowsPowerShell")
            .join("v1.0")
            .join("powershell.exe");

        std::fs::write(
            directory.path().join("sefy-plugin-file.cmd"),
            format!(
                "@echo off\r\n\
                 if \"%1\"==\"--manifest\" (type \"{manifest}\" & exit /b 0)\r\n\
                 \"{shell}\" -NoProfile -ExecutionPolicy Bypass -File \"{script}\"\r\n",
                manifest = manifest_path.display(),
                script = script.display(),
                shell = shell.display(),
            ),
        )
        .unwrap();
    }

    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        let path = directory.path().join("sefy-plugin-file");
        std::fs::write(
            &path,
            format!(
                r#"#!/bin/sh
if [ "$1" = "--manifest" ]; then
  printf '%s' '{manifest}'
  exit 0
fi
REQUEST=$(cat)
FILE=$(printf '%s' "$REQUEST" | sed 's/.*"file":"//; s/".*//')
printf '%s' "$REQUEST" | sed 's/.*"name":"//; s/".*//' > '{seen}'
case "$REQUEST" in
  *'"operation":"push"'*) cp "$FILE" '{remote}' ;;
  *) cp '{remote}' "$FILE" ;;
esac
"#
            ),
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    directory
}

/// The `name` the transport was handed on its last call.
fn name_the_transport_saw(transports: &Path) -> String {
    std::fs::read_to_string(transports.join("seen-name.txt"))
        .expect("the transport must have been called")
        .trim()
        .to_owned()
}

/// A second transport, so a test can ask what happens when the choice is not
/// obvious. It describes itself and does nothing else.
fn second_transport(directory: &Path) {
    let manifest =
        r#"{"protocol_version":1,"name":"other","version":"0.1.0","operations":["push","pull"]}"#;

    #[cfg(windows)]
    {
        let manifest_path = directory.join("other.json");
        std::fs::write(&manifest_path, manifest).unwrap();
        std::fs::write(
            directory.join("sefy-plugin-other.cmd"),
            format!(
                "@echo off\r\nif \"%1\"==\"--manifest\" (type \"{}\")\r\n",
                manifest_path.display()
            ),
        )
        .unwrap();
    }

    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        let path = directory.join("sefy-plugin-other");
        std::fs::write(
            &path,
            format!(
                "#!/bin/sh\nif [ \"$1\" = \"--manifest\" ]; then\n  printf '%s' '{manifest}'\nfi\n"
            ),
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}

/// `sefy` pointed at this vault *and* able to see only these transports.
fn sefy_with_transport(fixture: &Fixture, transports: &Path) -> Command {
    let mut command = fixture.sefy();
    command
        .env("PATH", transports)
        .env("APPDATA", transports)
        .env("XDG_DATA_HOME", transports)
        .env("HOME", transports)
        .env_remove("SEFY_TRANSPORT")
        .env_remove("SEFY_REMOTE_NAME");
    command
}

#[test]
fn a_push_sends_the_file_and_a_pull_brings_the_other_side_back() {
    let here = Fixture::with_vault();
    add_note(&here, "my note", "from here", &[]);

    // A second machine's copy of the same vault: created from this one's file,
    // so the two share item identities and a fold is a merge, not an import.
    let there = Fixture::empty();
    std::fs::copy(&here.path, &there.path).unwrap();
    add_note(&there, "their note", "from over there", &[]);

    let remote = here.directory().join("remote.bin");
    let transports = transport_directory(&remote);

    // The other machine publishes first.
    sefy_with_transport(&there, transports.path())
        .arg("push")
        .assert()
        .success()
        .stdout(contains("pushed"));

    sefy_with_transport(&here, transports.path())
        .arg("pull")
        .assert()
        .success()
        .stdout(contains("1 added"));

    sefy_with_transport(&here, transports.path())
        .arg("ls")
        .assert()
        .success()
        .stdout(contains("my note").and(contains("their note")));
}

#[test]
fn a_sync_leaves_both_sides_holding_everything() {
    let here = Fixture::with_vault();
    add_note(&here, "my note", "from here", &[]);

    let there = Fixture::empty();
    std::fs::copy(&here.path, &there.path).unwrap();
    add_note(&there, "their note", "from over there", &[]);

    let remote = here.directory().join("remote.bin");
    let transports = transport_directory(&remote);

    sefy_with_transport(&there, transports.path())
        .arg("push")
        .assert()
        .success();

    sefy_with_transport(&here, transports.path())
        .arg("sync")
        .assert()
        .success()
        .stdout(contains("1 added"));

    // A sync publishes what it just folded together, so the other machine gets
    // everything with a plain pull.
    sefy_with_transport(&there, transports.path())
        .arg("pull")
        .assert()
        .success();

    sefy_with_transport(&there, transports.path())
        .arg("ls")
        .assert()
        .success()
        .stdout(contains("my note").and(contains("their note")));
}

#[test]
fn what_reaches_the_remote_never_contains_a_secret_in_the_clear() {
    let fixture = Fixture::with_vault();
    add_note(&fixture, "bank", "SECRETVALUE", &[]);
    let remote = fixture.directory().join("remote.bin");
    let transports = transport_directory(&remote);

    sefy_with_transport(&fixture, transports.path())
        .arg("push")
        .assert()
        .success();

    let carried = std::fs::read(&remote).unwrap();
    assert!(
        !carried
            .windows(b"SECRETVALUE".len())
            .any(|window| window == b"SECRETVALUE"),
        "the remote copy holds ciphertext"
    );
}

#[test]
fn with_no_transport_installed_the_error_says_how_to_install_one() {
    let fixture = Fixture::with_vault();
    let empty = tempfile::tempdir().unwrap();

    sefy_with_transport(&fixture, empty.path())
        .arg("push")
        .assert()
        .failure()
        .stderr(contains("no usable transport").and(contains("sefy-plugin-")));
}

#[test]
fn with_several_installed_sefy_asks_which_rather_than_guessing() {
    let fixture = Fixture::with_vault();
    let remote = fixture.directory().join("remote.bin");
    let transports = transport_directory(&remote);
    second_transport(transports.path());

    sefy_with_transport(&fixture, transports.path())
        .arg("push")
        .assert()
        .failure()
        // Choosing on its own would mean deciding where somebody's vault goes.
        .stderr(
            contains("--transport")
                .and(contains("file"))
                .and(contains("other")),
        );

    // Naming one settles it.
    sefy_with_transport(&fixture, transports.path())
        .args(["push", "--transport", "file"])
        .assert()
        .success();
}

#[test]
fn a_transport_that_is_not_installed_is_named_in_the_error() {
    let fixture = Fixture::with_vault();
    let remote = fixture.directory().join("remote.bin");
    let transports = transport_directory(&remote);

    sefy_with_transport(&fixture, transports.path())
        .args(["push", "--transport", "nowhere"])
        .assert()
        .failure()
        .stderr(contains("nowhere").and(contains("plugin list")));
}

#[test]
fn the_remote_name_is_what_the_transport_is_told_to_call_it() {
    let fixture = Fixture::with_vault();
    let remote = fixture.directory().join("remote.bin");
    let transports = transport_directory(&remote);

    sefy_with_transport(&fixture, transports.path())
        .args(["push", "--name", "work-laptop"])
        .assert()
        .success()
        .stdout(contains("work-laptop"));

    // What sefy printed is not the point: the name has to reach the transport,
    // which is what decides where the copy is stored.
    assert_eq!(name_the_transport_saw(transports.path()), "work-laptop");

    // And without --name, the documented default is what travels.
    sefy_with_transport(&fixture, transports.path())
        .arg("push")
        .assert()
        .success();
    assert_eq!(name_the_transport_saw(transports.path()), "vault");
}

#[test]
fn a_pull_under_a_different_remote_password_is_asked_for_separately() {
    let here = Fixture::with_vault();

    // The copy on the other side is a different vault under its own password —
    // the case --remote-password-env exists for.
    let there = Fixture::empty();
    there
        .sefy()
        .env("SEFY_TEST_PASSWORD", "another password")
        .arg("init")
        .assert()
        .success();
    let mut add = there.sefy();
    add.env("SEFY_TEST_PASSWORD", "another password")
        .args(["add", "note", "their note", "--text", "from over there"])
        .assert()
        .success();

    let remote = here.directory().join("remote.bin");
    std::fs::copy(&there.path, &remote).unwrap();
    let transports = transport_directory(&remote);

    // Without being told, sefy tries this vault's password and says so plainly.
    sefy_with_transport(&here, transports.path())
        .arg("pull")
        .assert()
        .failure()
        .stderr(contains("wrong password"));

    sefy_with_transport(&here, transports.path())
        .env("SEFY_REMOTE_PASSWORD", "another password")
        .args(["pull", "--remote-password-env", "SEFY_REMOTE_PASSWORD"])
        .assert()
        .success()
        .stdout(contains("1 added"));
}
