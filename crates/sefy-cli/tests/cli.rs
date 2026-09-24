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
            "login",
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
        .args(["add", "login", "mail", "--login", "someone"])
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
        .args(["find", "bank", "--kind", "login"])
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
        .args(["edit", "a note", "--set", "login=someone"])
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
            "login",
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
            "login",
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
        .stderr(contains("login"));
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

/// The directory sefy looks in, given a stand-in for the per-user data
/// directory. Created on the way, since the fixtures are written into it.
fn plugins_inside(data: &Path) -> PathBuf {
    // The same resolution sefy does, so the fixture is found the way a real
    // transport is. Not a shared helper with the library: a test that asked the
    // code under test where to put the fixture would agree with it even when
    // both are wrong.
    #[cfg(target_os = "macos")]
    let base = data.join("Library").join("Application Support");
    #[cfg(not(target_os = "macos"))]
    let base = data.to_path_buf();

    let directory = base.join("sefy").join("plugins");
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

/// Writes a transport whose "remote" is another file on this machine.
///
/// The returned directory stands in for the per-user data directory, and the
/// executable goes into the `sefy/plugins` subdirectory sefy documents — the
/// same route a real installation takes.
///
/// It reads the path to move from the request on stdin — the same barrier the
/// core tests rely on, and the reason a fixture told the path some other way
/// would prove nothing.
fn transport_directory(remote: &Path) -> tempfile::TempDir {
    let data = tempfile::tempdir().unwrap();
    let directory = plugins_inside(data.path());
    let manifest =
        r#"{"protocol_version":1,"name":"file","version":"0.1.0","operations":["push","pull"]}"#;
    let remote = remote.display().to_string();
    // Whatever `name` arrives in the request is recorded here, so a test can
    // check what sefy told the transport rather than only what it printed.
    let seen = directory.join("seen-name.txt").display().to_string();

    #[cfg(windows)]
    {
        let manifest_path = directory.join("manifest.json");
        std::fs::write(&manifest_path, manifest).unwrap();

        let script = directory.join("transport.ps1");
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

        // PowerShell by absolute path: the PATH these tests hand over is
        // trimmed, and a bare `powershell` failing to start would surface as
        // "the plugin failed" — pointing at sefy rather than the fixture.
        let shell = PathBuf::from(std::env::var_os("SystemRoot").unwrap())
            .join("System32")
            .join("WindowsPowerShell")
            .join("v1.0")
            .join("powershell.exe");

        std::fs::write(
            directory.join("sefy-plugin-file.cmd"),
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
        let path = directory.join("sefy-plugin-file");
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

    data
}

/// The `name` the transport was handed on its last call.
fn name_the_transport_saw(data: &Path) -> String {
    std::fs::read_to_string(plugins_inside(data).join("seen-name.txt"))
        .expect("the transport must have been called")
        .trim()
        .to_owned()
}

/// A second transport, so a test can ask what happens when the choice is not
/// obvious. It describes itself and does nothing else.
fn second_transport(data: &Path) {
    let directory = plugins_inside(data);
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
///
/// The transports are found through the data directory rather than by cutting
/// `PATH` down to their own: these fixtures are shell scripts, and a `PATH`
/// holding nothing else leaves them without `cp` or `sed` — which fails as
/// "the plugin failed", pointing at sefy rather than at the fixture.
/// `PATH` keeps the system directories and loses the user ones, so a transport
/// actually installed on the machine running the tests still cannot answer.
fn sefy_with_transport(fixture: &Fixture, transports: &Path) -> Command {
    let mut command = fixture.sefy();
    command
        .env("PATH", system_path())
        .env("APPDATA", transports)
        .env("XDG_DATA_HOME", transports)
        .env("HOME", transports)
        .env_remove("SEFY_TRANSPORT")
        .env_remove("SEFY_REMOTE_NAME");
    command
}

/// The directories a plain shell needs, and nothing a plugin could be installed
/// into by hand.
fn system_path() -> std::ffi::OsString {
    #[cfg(windows)]
    {
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        std::ffi::OsString::from(format!(
            r"{root}\System32;{root};{root}\System32\WindowsPowerShell\v1.0"
        ))
    }

    #[cfg(not(windows))]
    {
        std::ffi::OsString::from("/usr/bin:/bin:/usr/sbin:/sbin")
    }
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

#[test]
fn the_fixture_transport_is_where_sefy_looks_for_one() {
    // A guard for the tests below rather than for the product: when the fixture
    // lands somewhere sefy does not search, every transport test fails with
    // "no usable transport installed" — which reads as a bug in the selection
    // code rather than a fixture in the wrong directory. macOS resolves the
    // data directory through ~/Library/Application Support, and getting that
    // wrong is exactly how this was discovered.
    let fixture = Fixture::with_vault();
    let transports = transport_directory(&fixture.directory().join("remote.bin"));

    sefy_with_transport(&fixture, transports.path())
        .args(["plugin", "list"])
        .assert()
        .success()
        .stdout(contains("file").and(contains("pull")).and(contains("push")));
}

/// Writes an item whose kind this build does not know, the way a newer sefy
/// would: a row in `items` and its contents in a table of its own.
fn add_item_of_a_future_kind(fixture: &Fixture, title: &str) {
    let file = std::fs::read(&fixture.path).unwrap();
    let database = sefy_core::format::decode(MASTER.as_bytes(), &file).unwrap();
    let connection = sefy_core::db::load(&database).unwrap();

    connection
        .execute(
            "INSERT INTO items (uuid, title, kind, created_at, updated_at)
             VALUES ('11111111-2222-4333-8444-555555555555', ?1, 'passport', 1000, 1000)",
            [title],
        )
        .unwrap();
    let id = connection.last_insert_rowid();
    connection
        .execute_batch("CREATE TABLE passports (item_id INTEGER NOT NULL, number TEXT NOT NULL)")
        .unwrap();
    connection
        .execute(
            "INSERT INTO passports (item_id, number) VALUES (?1, '4111')",
            [id],
        )
        .unwrap();

    let dumped = sefy_core::db::dump(&connection).unwrap();
    std::fs::write(
        &fixture.path,
        sefy_core::format::encode(MASTER.as_bytes(), &dumped).unwrap(),
    )
    .unwrap();
}

#[test]
fn an_item_from_a_newer_sefy_does_not_break_the_listing() {
    let fixture = Fixture::with_vault();
    add_note(&fixture, "shed", "combination 4815", &[]);
    add_item_of_a_future_kind(&fixture, "my passport");

    // The whole point: one unreadable item used to take the listing down with
    // it, reporting "no item with id 2" about an item sitting right there.
    fixture
        .sefy()
        .arg("ls")
        .assert()
        .success()
        .stdout(contains("shed").and(contains("my passport")))
        .stdout(contains("needs a newer sefy"));
}

#[test]
fn reading_an_item_from_a_newer_sefy_explains_rather_than_denies_it() {
    let fixture = Fixture::with_vault();
    add_item_of_a_future_kind(&fixture, "my passport");

    fixture
        .sefy()
        .args(["show", "my passport"])
        .assert()
        .success()
        .stdout(contains("passport").and(contains("does not know")));

    fixture
        .sefy()
        .args(["get", "my passport", "--stdout"])
        .assert()
        .failure()
        .stderr(contains("upgrade to read it"))
        // Each line of the message starts at the margin, not indented by the
        // source it was written in.
        .stderr(contains("does not know\nit was written"));
}

#[test]
fn an_item_from_a_newer_sefy_can_still_be_retitled() {
    let fixture = Fixture::with_vault();
    add_item_of_a_future_kind(&fixture, "my passport");

    fixture
        .sefy()
        .args(["edit", "my passport", "--title", "travel passport"])
        .assert()
        .success();
    fixture
        .sefy()
        .arg("ls")
        .assert()
        .success()
        .stdout(contains("travel passport"));

    // Rewriting its contents is refused: this build cannot read what is there.
    fixture
        .sefy()
        .args(["edit", "travel passport", "--text", "nope"])
        .assert()
        .failure()
        .stderr(contains("only --title and tags"));
}

/// Adds a login with every option filled in, using environment variables for
/// the two secrets a login can carry.
fn add_full_login(fixture: &Fixture, title: &str) {
    fixture
        .sefy()
        .env("ITEM_PASSWORD", "hunter2")
        .args([
            "add",
            "login",
            title,
            "--login",
            "someone",
            "--url",
            "https://example.invalid",
            "--totp",
            // Spaced and lower-cased, the way a setup page prints a key for
            // typing: stored as the key, without the dressing.
            "jbsw y3dp ehpk 3pxp",
            "--notes",
            "backup codes in the drawer",
            "--item-password-env",
            "ITEM_PASSWORD",
        ])
        .assert()
        .success();
}

#[test]
fn a_login_round_trips_every_field_it_was_given() {
    let fixture = Fixture::with_vault();
    add_full_login(&fixture, "mail");

    // No --field: the kind's own secret, the password.
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

    fixture
        .sefy()
        .args(["get", "mail", "--field", "totp", "--stdout"])
        .assert()
        .success()
        .stdout("JBSWY3DPEHPK3PXP\n");
}

#[test]
fn a_login_option_left_out_produces_no_field_at_all() {
    let fixture = Fixture::with_vault();
    fixture
        .sefy()
        .env("ITEM_PASSWORD", "hunter2")
        .args([
            "add",
            "login",
            "bare",
            "--login",
            "someone",
            "--item-password-env",
            "ITEM_PASSWORD",
        ])
        .assert()
        .success();

    fixture
        .sefy()
        .args(["show", "bare"])
        .assert()
        .success()
        .stdout(contains("login:"))
        .stdout(contains("password:"))
        // Not asked for, so not a field at all — not an empty one.
        .stdout(contains("url:").not())
        .stdout(contains("totp:").not())
        .stdout(contains("notes:").not());
}

#[test]
fn a_card_round_trips_and_get_with_no_field_gives_the_number() {
    let fixture = Fixture::with_vault();
    fixture
        .sefy()
        .env("CARD_NUMBER", "4111111111111111")
        .env("CARD_CVV", "123")
        .env("CARD_PIN", "4815")
        .args([
            "add",
            "card",
            "wallet",
            "--holder",
            "A Cardholder",
            "--expiry",
            "12/30",
            "--number-env",
            "CARD_NUMBER",
            "--cvv-env",
            "CARD_CVV",
            "--pin-env",
            "CARD_PIN",
        ])
        .assert()
        .success();

    // A card has no password field; its own secret is the number.
    fixture
        .sefy()
        .args(["get", "wallet", "--stdout"])
        .assert()
        .success()
        .stdout(contains("4111111111111111"));

    fixture
        .sefy()
        .args(["get", "wallet", "--field", "cvv", "--stdout"])
        .assert()
        .success()
        .stdout(contains("123"));

    fixture
        .sefy()
        .args(["get", "wallet", "--field", "pin", "--stdout"])
        .assert()
        .success()
        .stdout(contains("4815"));
}

#[test]
fn no_cvv_and_no_pin_leave_those_fields_out_of_the_card() {
    let fixture = Fixture::with_vault();
    fixture
        .sefy()
        .env("CARD_NUMBER", "4111111111111111")
        .args([
            "add",
            "card",
            "bare card",
            "--number-env",
            "CARD_NUMBER",
            "--no-cvv",
            "--no-pin",
        ])
        .assert()
        .success();

    fixture
        .sefy()
        .args(["show", "bare card"])
        .assert()
        .success()
        .stdout(contains("number:"))
        .stdout(contains("cvv:").not())
        .stdout(contains("pin:").not());
}

#[test]
fn an_ssh_key_stores_a_trimmed_public_key_and_the_private_key_comes_back_exact() {
    let fixture = Fixture::with_vault();
    let private_path = fixture.directory().join("id_ed25519");
    let public_path = fixture.directory().join("id_ed25519.pub");
    std::fs::write(
        &private_path,
        "-----BEGIN PRIVATE KEY-----\nabc\n-----END-----\n",
    )
    .unwrap();
    // A trailing newline is exactly what `ssh-keygen` leaves — trimming it is
    // the point of this test, not an accident of the fixture.
    std::fs::write(&public_path, "ssh-ed25519 AAAAC3 comment\n").unwrap();

    fixture
        .sefy()
        .args(["add", "ssh-key", "prod server", "--private-key"])
        .arg(&private_path)
        .args(["--host", "prod.example.invalid", "--no-passphrase"])
        .assert()
        .success();

    fixture
        .sefy()
        .args(["show", "prod server"])
        .assert()
        .success()
        .stdout(contains("ssh-ed25519 AAAAC3 comment"));

    fixture
        .sefy()
        .args(["get", "prod server", "--field", "private-key", "--stdout"])
        .assert()
        .success()
        .stdout(contains(
            "-----BEGIN PRIVATE KEY-----\nabc\n-----END-----\n",
        ));
}

#[test]
fn an_ssh_key_with_no_pub_file_beside_it_still_succeeds_and_has_no_public_key_field() {
    let fixture = Fixture::with_vault();
    let private_path = fixture.directory().join("lonely_key");
    std::fs::write(&private_path, "the private half only").unwrap();

    fixture
        .sefy()
        .args(["add", "ssh-key", "lonely", "--private-key"])
        .arg(&private_path)
        .arg("--no-passphrase")
        .assert()
        .success();

    fixture
        .sefy()
        .args(["show", "lonely"])
        .assert()
        .success()
        .stdout(contains("private-key:"))
        .stdout(contains("public-key:").not());
}

#[test]
fn get_field_on_a_name_the_record_lacks_lists_what_it_does_have() {
    let fixture = Fixture::with_vault();
    add_full_login(&fixture, "mail");

    fixture
        .sefy()
        .args(["get", "mail", "--field", "nonexistent"])
        .assert()
        .failure()
        .stderr(contains("nonexistent"))
        .stderr(contains("login"))
        .stderr(contains("password"))
        .stderr(contains("url"))
        .stderr(contains("totp"))
        .stderr(contains("notes"));
}

#[test]
fn show_hides_every_secret_field_and_names_it_in_the_hint() {
    let fixture = Fixture::with_vault();
    add_full_login(&fixture, "mail");

    fixture
        .sefy()
        .args(["show", "mail"])
        .assert()
        .success()
        .stdout(contains("hunter2").not())
        .stdout(contains("JBSWY3DP").not())
        .stdout(contains("<hidden — use sefy get --field password>"))
        .stdout(contains("<hidden — sefy otp gives the code>"));
}

#[test]
fn edit_set_changes_an_existing_fields_value() {
    let fixture = Fixture::with_vault();
    add_full_login(&fixture, "mail");

    fixture
        .sefy()
        .args(["edit", "mail", "--set", "login=someone-else"])
        .assert()
        .success();

    fixture
        .sefy()
        .args(["get", "mail", "--field", "login", "--stdout"])
        .assert()
        .success()
        .stdout(contains("someone-else"));
}

#[test]
fn edit_set_adds_a_field_the_record_did_not_have_and_it_is_public() {
    let fixture = Fixture::with_vault();
    let private_path = fixture.directory().join("id_ed25519");
    std::fs::write(&private_path, "private half").unwrap();
    fixture
        .sefy()
        .args(["add", "ssh-key", "box", "--private-key"])
        .arg(&private_path)
        .arg("--no-passphrase")
        .assert()
        .success();

    fixture
        .sefy()
        .args(["edit", "box", "--set", "fingerprint=abc"])
        .assert()
        .success();

    // A name the template has no opinion about lands public — visible in
    // `show`, not "<hidden — ...>".
    fixture
        .sefy()
        .args(["show", "box"])
        .assert()
        .success()
        .stdout(contains("fingerprint: abc"));
}

#[test]
fn edit_set_on_an_existing_secret_field_keeps_it_secret() {
    let fixture = Fixture::with_vault();
    add_full_login(&fixture, "mail");

    fixture
        .sefy()
        .args(["edit", "mail", "--set", "password=newpassword"])
        .assert()
        .success();

    // The value on the command line is public by default, but `password`
    // already existed as a secret field — changing its value must not make it
    // readable from `show`.
    fixture
        .sefy()
        .args(["show", "mail"])
        .assert()
        .success()
        .stdout(contains("newpassword").not())
        .stdout(contains("<hidden — use sefy get --field password>"));

    fixture
        .sefy()
        .args(["get", "mail", "--stdout"])
        .assert()
        .success()
        .stdout(contains("newpassword"));
}

#[test]
fn edit_unset_removes_a_field_and_refuses_a_name_that_is_not_there() {
    let fixture = Fixture::with_vault();
    add_full_login(&fixture, "mail");

    fixture
        .sefy()
        .args(["edit", "mail", "--unset", "notes"])
        .assert()
        .success();

    fixture
        .sefy()
        .args(["show", "mail"])
        .assert()
        .success()
        .stdout(contains("notes:").not());

    fixture
        .sefy()
        .args(["edit", "mail", "--unset", "notes"])
        .assert()
        .failure()
        .stderr(contains("no field named"))
        .stderr(contains("login"));
}

#[test]
fn edit_unset_of_every_field_is_refused() {
    let fixture = Fixture::with_vault();
    fixture
        .sefy()
        .env("ITEM_PASSWORD", "hunter2")
        .args([
            "add",
            "login",
            "bare",
            "--login",
            "someone",
            "--item-password-env",
            "ITEM_PASSWORD",
        ])
        .assert()
        .success();

    fixture
        .sefy()
        .args(["edit", "bare", "--unset", "login", "--unset", "password"])
        .assert()
        .failure()
        .stderr(contains("cannot be left without any field"));

    // Refused, so both fields are still there.
    fixture
        .sefy()
        .args(["show", "bare"])
        .assert()
        .success()
        .stdout(contains("login:"));
}

#[test]
fn a_malformed_set_without_an_equals_sign_is_an_error() {
    let fixture = Fixture::with_vault();
    add_full_login(&fixture, "mail");

    fixture
        .sefy()
        .args(["edit", "mail", "--set", "loginsomeone"])
        .assert()
        .failure()
        .stderr(contains("NAME=VALUE"));
}

#[test]
fn set_on_a_note_is_refused_as_a_records_only_option() {
    let fixture = Fixture::with_vault();
    add_note(&fixture, "a note", "text", &[]);

    fixture
        .sefy()
        .args(["edit", "a note", "--set", "anything=value"])
        .assert()
        .failure()
        .stderr(contains("--set, --set-secret and --unset apply to records"));
}

#[test]
fn ls_kind_shows_only_that_kind_and_credential_is_an_alias_for_login() {
    let fixture = Fixture::with_vault();
    add_full_login(&fixture, "mail");
    fixture
        .sefy()
        .env("CARD_NUMBER", "4111111111111111")
        .args([
            "add",
            "card",
            "wallet",
            "--number-env",
            "CARD_NUMBER",
            "--no-cvv",
            "--no-pin",
        ])
        .assert()
        .success();

    fixture
        .sefy()
        .args(["ls", "--kind", "card"])
        .assert()
        .success()
        .stdout(contains("wallet"))
        .stdout(contains("mail").not());

    // `credential` is the old spelling of `login`, kept as an alias.
    fixture
        .sefy()
        .args(["ls", "--kind", "credential"])
        .assert()
        .success()
        .stdout(contains("mail"))
        .stdout(contains("wallet").not());
}

#[test]
fn add_credential_is_still_accepted_and_produces_a_login() {
    let fixture = Fixture::with_vault();
    fixture
        .sefy()
        .env("ITEM_PASSWORD", "hunter2")
        .args([
            "add",
            "credential",
            "legacy",
            "--login",
            "someone",
            "--item-password-env",
            "ITEM_PASSWORD",
        ])
        .assert()
        .success();

    fixture
        .sefy()
        .args(["show", "legacy"])
        .assert()
        .success()
        .stdout(contains("kind:        login"));
}

#[test]
fn set_secret_needs_a_terminal_it_does_not_have_in_a_script() {
    // `--set-secret` reads its value the way a password is read, and that
    // reader refuses anything but a real terminal — a script piping a value
    // in on stdin is exactly the case --set-secret is not for; --set exists
    // for that. This is the one edit-flag behaviour a piped test cannot
    // exercise past this point.
    let fixture = Fixture::with_vault();
    add_full_login(&fixture, "mail");

    fixture
        .sefy()
        .args(["edit", "mail", "--set-secret", "password"])
        .write_stdin("newpassword\n")
        .assert()
        .failure()
        .stderr(contains("not a terminal"));
}

#[test]
fn an_export_still_runs_with_an_item_from_a_newer_sefy_in_the_vault() {
    let fixture = Fixture::with_vault();
    add_note(&fixture, "shed", "combination 4815", &[]);
    add_item_of_a_future_kind(&fixture, "my passport");

    let output = fixture.directory().join("export.json");
    fixture
        .sefy()
        .args(["export", "--output"])
        .arg(&output)
        .arg("--i-know-this-writes-plaintext")
        .assert()
        .success();

    // "sefy can always get your data out" has to hold even when part of the
    // vault came from a version this one does not understand.
    let written = std::fs::read_to_string(&output).unwrap();
    assert!(written.contains("my passport"), "the item is in the export");
    assert!(
        written.contains("contents_not_exported"),
        "and the export says its contents could not be read"
    );
}

#[test]
fn status_reports_shape_and_never_contents() {
    let fixture = Fixture::with_vault();
    add_note(&fixture, "bank code", "vault code 1234", &["money"]);
    add_note(&fixture, "wifi", "hunter2", &["home", "money"]);

    let output = fixture.sefy().arg("status").assert().success();
    let printed = String::from_utf8(output.get_output().stdout.clone()).unwrap();

    // What it must say.
    assert!(printed.contains("2 items"), "item count: {printed}");
    assert!(printed.contains("2 note"), "kind breakdown: {printed}");
    // Two distinct tags across the two notes: `money` is on both.
    assert!(printed.contains("2 tags"), "tag count: {printed}");
    assert!(printed.contains("schema"), "schema version: {printed}");
    assert!(printed.contains("never"), "a fresh vault has not synced");

    // What it must not. A status is asked by someone checking they have the
    // right file, often with somebody else in the room - it answers about the
    // vault, never about what is in it.
    for secret in ["vault code 1234", "hunter2", "bank code", "wifi"] {
        assert!(
            !printed.contains(secret),
            "status printed {secret:?}, which is contents:\n{printed}"
        );
    }
}

#[test]
fn status_counts_a_kind_this_build_does_not_know() {
    // A vault written by a later sefy must still describe itself honestly,
    // rather than quietly leaving part of itself out of its own summary.
    let fixture = Fixture::with_vault();
    add_note(&fixture, "ordinary", "text", &[]);
    add_item_of_a_future_kind(&fixture, "my passport");

    let output = fixture.sefy().arg("status").assert().success();
    let printed = String::from_utf8(output.get_output().stdout.clone()).unwrap();

    assert!(printed.contains("2 items"), "both are counted: {printed}");
    assert!(
        printed.contains("passport"),
        "the unknown kind is named as stored: {printed}"
    );
}

#[test]
fn open_refuses_anything_that_is_not_a_web_address() {
    // The check that keeps a stored value from becoming something a launcher
    // acts on. No browser is started: each of these fails before that.
    let fixture = Fixture::with_vault();

    for (title, url) in [
        ("local", "file:///C:/Windows/System32/calc.exe"),
        ("script", "javascript:alert(1)"),
        ("settings", "ms-settings:privacy"),
        ("bare", "example.com"),
    ] {
        fixture
            .sefy()
            .args(["add", "login", title, "--login", "someone"])
            .args(["--item-password-env", "SEFY_TEST_PASSWORD"])
            .args(["--url", url])
            .assert()
            .success();

        fixture
            .sefy()
            .args(["open", title])
            .assert()
            .failure()
            .stderr(contains("web address"));
    }
}

#[test]
fn open_says_what_is_missing_rather_than_opening_nothing() {
    let fixture = Fixture::with_vault();
    add_note(&fixture, "a note", "text", &[]);
    fixture
        .sefy()
        .args(["add", "login", "no-site", "--login", "someone"])
        .args(["--item-password-env", "SEFY_TEST_PASSWORD"])
        .assert()
        .success();

    // A note has no site at all.
    fixture
        .sefy()
        .args(["open", "a note"])
        .assert()
        .failure()
        .stderr(contains("no site to open"));

    // A login without a url says which field is missing and how to add it.
    fixture
        .sefy()
        .args(["open", "no-site"])
        .assert()
        .failure()
        .stderr(contains("has no \"url\" field"))
        .stderr(contains("--set url="));
}

#[test]
fn the_picker_is_refused_off_a_terminal_rather_than_hanging() {
    // `sefy` with no command opens an interactive picker. Run from a script,
    // where there is nobody to type, it has to say so and exit - a prompt
    // drawn into a pipe waits for a keystroke that is never coming, which is a
    // hang with no explanation anywhere.
    let fixture = Fixture::with_vault();
    add_note(&fixture, "something", "text", &[]);

    fixture
        .sefy()
        .assert()
        .failure()
        .stderr(contains("not a terminal"))
        .stderr(contains("sefy ls"));
}

#[test]
fn find_lists_rather_than_picking_wherever_it_runs() {
    // `find` is what a script calls. It must be the same command in a terminal
    // and in a pipe: one name, one behaviour.
    let fixture = Fixture::with_vault();
    add_note(&fixture, "bank code", "text", &["money"]);
    add_note(&fixture, "wifi", "text", &["home"]);

    fixture
        .sefy()
        .args(["find", "--tag", "money"])
        .assert()
        .success()
        .stdout(contains("bank code"))
        .stdout(contains("wifi").not());
}

/// `sefy gen` with no vault anywhere in sight.
fn sefy_without_a_vault() -> Command {
    let mut command = Command::cargo_bin("sefy").unwrap();
    command.env_remove("SEFY_VAULT");
    command
}

/// What `sefy gen --stdout` printed on stdout, which should be the secret alone.
fn generated(command: &mut Command) -> String {
    let output = command
        .arg("--stdout")
        .assert()
        .success()
        .get_output()
        .clone();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines.len(),
        1,
        "stdout carries more than the secret: {stdout:?}"
    );
    lines[0].to_owned()
}

#[test]
fn gen_needs_no_vault_and_prints_the_secret_alone() {
    let password = generated(sefy_without_a_vault().arg("gen"));
    assert_eq!(password.chars().count(), 20);

    // The description is still there - on stderr, out of the pipe's way.
    sefy_without_a_vault()
        .args(["gen", "--stdout"])
        .assert()
        .success()
        .stderr(contains("bits of entropy").and(contains("strength")));
}

#[test]
fn gen_does_not_touch_a_vault_it_was_not_asked_to_save_into() {
    let fixture = Fixture::empty();
    generated(fixture.sefy().arg("gen"));
    assert!(!fixture.path.exists());
}

#[test]
fn gen_follows_the_policy_it_was_given() {
    let password = generated(sefy_without_a_vault().args([
        "gen",
        "--length",
        "64",
        "--no-symbols",
        "--no-uppercase",
    ]));
    assert_eq!(password.len(), 64);
    assert!(
        password
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    );
}

#[test]
fn gen_makes_a_passphrase_from_the_list_asked_for() {
    let english = generated(sefy_without_a_vault().args(["gen", "--words", "5"]));
    assert_eq!(english.split('-').count(), 5);

    let russian = generated(sefy_without_a_vault().args([
        "gen",
        "--words",
        "4",
        "--lang",
        "ru",
        "--separator",
        " ",
    ]));
    let words: Vec<&str> = russian.split(' ').collect();
    assert_eq!(words.len(), 4);
    assert!(
        words
            .iter()
            .all(|word| word.chars().all(|c| ('а'..='я').contains(&c)))
    );
}

#[test]
fn gen_refuses_options_that_belong_to_another_recipe() {
    for arguments in [
        &["gen", "--words", "4", "--no-symbols"][..],
        &["gen", "--words", "4", "--length", "30"],
        &["gen", "--pronounceable", "--no-digits"],
        &["gen", "--lang", "ru"],
        &["gen", "--login", "someone"],
    ] {
        sefy_without_a_vault().args(arguments).assert().failure();
    }
}

#[test]
fn gen_save_stores_the_very_password_it_hands_over() {
    let fixture = Fixture::with_vault();
    let password = generated(fixture.sefy().args([
        "gen",
        "--save",
        "forum",
        "--login",
        "someone@example.com",
        "--url",
        "https://forum.example.com",
        "--tag",
        "web",
    ]));

    fixture
        .sefy()
        .args(["get", "forum", "--stdout"])
        .assert()
        .success()
        .stdout(format!("{password}\n"));
    fixture
        .sefy()
        .args(["show", "forum"])
        .assert()
        .success()
        .stdout(
            contains("kind:")
                .and(contains("login"))
                .and(contains("someone@example.com"))
                .and(contains("https://forum.example.com"))
                .and(contains("web"))
                .and(contains(password.as_str()).not()),
        );
}

#[test]
fn gen_save_with_the_wrong_password_hands_over_nothing() {
    // A password the vault did not take must not reach the user either: it
    // would go into a site's form and exist nowhere else.
    let fixture = Fixture::with_vault();
    fixture
        .sefy()
        .env("SEFY_TEST_PASSWORD", "not the master password")
        .args(["gen", "--save", "forum", "--stdout"])
        .assert()
        .failure()
        .stdout("");
}

// ---------------------------------------------------------------------------
// One-time passwords

const OTP_KEY: &str = "JBSWY3DPEHPK3PXP";

/// Adds a login with no one-time password key yet.
fn add_plain_login(fixture: &Fixture, title: &str) {
    fixture
        .sefy()
        .env("ITEM_PASSWORD", "hunter2")
        .args(["add", "login", title, "--login", "someone"])
        .args(["--item-password-env", "ITEM_PASSWORD"])
        .assert()
        .success();
}

/// The codes `key` makes around now: the one before the command ran and the
/// one after, since a window can close while the command is running.
fn codes_around_now(key: &str, run: impl FnOnce() -> String) -> (String, [String; 2]) {
    let totp = sefy_core::Totp::parse(key).unwrap();
    let now = || {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    };
    let before = totp.code_at(now());
    let output = run();
    let after = totp.code_at(now());
    (output, [before, after])
}

#[test]
fn otp_set_stores_the_key_and_answers_with_the_first_code() {
    let fixture = Fixture::with_vault();
    add_plain_login(&fixture, "forge");

    let (stdout, expected) = codes_around_now(OTP_KEY, || {
        let output = fixture
            .sefy()
            .env("OTP_KEY", "jbsw-y3dp ehpk-3pxp")
            .args(["otp", "forge", "--key-env", "OTP_KEY", "--stdout"])
            .assert()
            .success()
            // The note about storing goes to stderr: a pipe gets the code.
            .stderr(contains("stored the one-time password key of \"forge\""))
            .get_output()
            .stdout
            .clone();
        String::from_utf8(output).unwrap()
    });
    let code = stdout.trim_end();
    assert_eq!(stdout, format!("{code}\n"), "stdout carries the code alone");
    assert!(
        expected.contains(&code.to_owned()),
        "{code} not in {expected:?}"
    );

    // The key is on the record, as the key.
    fixture
        .sefy()
        .args(["get", "forge", "--field", "totp", "--stdout"])
        .assert()
        .success()
        .stdout(format!("{OTP_KEY}\n"));

    // And a later call makes codes from it without being told again.
    let (stdout, expected) = codes_around_now(OTP_KEY, || {
        let output = fixture
            .sefy()
            .args(["otp", "forge", "--stdout"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        String::from_utf8(output).unwrap()
    });
    assert!(expected.contains(&stdout.trim_end().to_owned()));
}

#[test]
fn otp_follows_the_parameters_of_a_link() {
    let fixture = Fixture::with_vault();
    add_plain_login(&fixture, "bank portal");
    let link = format!(
        "otpauth://totp/Example%20Bank:someone?secret={OTP_KEY}&issuer=Example%20Bank&digits=8&algorithm=SHA256"
    );

    let (stdout, expected) = codes_around_now(&link, || {
        let output = fixture
            .sefy()
            .env("OTP_KEY", &link)
            .args(["otp", "bank portal", "--key-env", "OTP_KEY", "--stdout"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        String::from_utf8(output).unwrap()
    });
    let code = stdout.trim_end();
    assert_eq!(code.len(), 8, "{code}");
    assert!(expected.contains(&code.to_owned()));

    // A link is kept as given: its issuer and parameters are the site's words.
    fixture
        .sefy()
        .args(["get", "bank portal", "--field", "totp", "--stdout"])
        .assert()
        .success()
        .stdout(format!("{link}\n"));
}

#[test]
fn otp_refuses_what_is_not_a_key_and_leaves_the_record_alone() {
    let fixture = Fixture::with_vault();
    add_plain_login(&fixture, "forge");

    fixture
        .sefy()
        .env("OTP_KEY", "hunter2-is-not-base32!")
        .args(["otp", "forge", "--key-env", "OTP_KEY", "--stdout"])
        .assert()
        .failure()
        .stderr(contains("not a one-time password key"))
        .stderr(contains("hunter2").not())
        .stdout("");

    fixture
        .sefy()
        .args(["get", "forge", "--field", "totp", "--stdout"])
        .assert()
        .failure();
}

#[test]
fn otp_refuses_a_counter_based_key_by_name() {
    let fixture = Fixture::with_vault();
    add_plain_login(&fixture, "forge");
    fixture
        .sefy()
        .env(
            "OTP_KEY",
            format!("otpauth://hotp/x?secret={OTP_KEY}&counter=3"),
        )
        .args(["otp", "forge", "--key-env", "OTP_KEY"])
        .assert()
        .failure()
        .stderr(contains("HOTP"));
}

#[test]
fn otp_on_a_login_without_a_key_says_how_to_store_one() {
    let fixture = Fixture::with_vault();
    add_plain_login(&fixture, "forge");
    fixture
        .sefy()
        .args(["otp", "forge", "--stdout"])
        .assert()
        .failure()
        .stderr(contains("has no one-time password key"))
        .stderr(contains("sefy otp 1 --set"));
}

#[test]
fn otp_on_a_note_is_refused() {
    let fixture = Fixture::with_vault();
    add_note(&fixture, "diary", "dear diary", &[]);
    fixture
        .sefy()
        .args(["otp", "diary", "--stdout"])
        .assert()
        .failure()
        .stderr(contains("is a note"));
}

#[test]
fn otp_qr_draws_only_on_a_terminal() {
    let fixture = Fixture::with_vault();
    add_full_login(&fixture, "mail");
    fixture
        .sefy()
        .args(["otp", "mail", "--qr"])
        .assert()
        .failure()
        .stderr(contains("only on a terminal"))
        .stdout(contains(OTP_KEY).not())
        .stdout(contains("\u{2580}").not());
}

#[test]
fn a_key_that_cannot_make_a_code_is_refused_on_the_way_in() {
    let fixture = Fixture::with_vault();

    fixture
        .sefy()
        .env("ITEM_PASSWORD", "hunter2")
        .args(["add", "login", "mail", "--login", "someone"])
        .args(["--totp", "hunter1!", "--item-password-env", "ITEM_PASSWORD"])
        .assert()
        .failure()
        .stderr(contains("not a one-time password key"));

    add_plain_login(&fixture, "forge");
    fixture
        .sefy()
        .args(["edit", "forge", "--set", "totp=1234"])
        .assert()
        .failure()
        .stderr(contains("not a one-time password key"));
}

// ---------------------------------------------------------------------------
// Filling in a form

#[test]
fn fill_needs_a_terminal_to_wait_on_and_says_what_to_do_instead() {
    let fixture = Fixture::with_vault();
    add_full_login(&fixture, "mail");
    fixture
        .sefy()
        .args(["fill", "mail"])
        .assert()
        .failure()
        .stderr(contains("waits for Enter"))
        .stderr(contains("not a terminal\ntake one field at a time"))
        .stderr(contains("sefy get 1 --field NAME --stdout"))
        .stdout("");
}

#[test]
fn fill_refuses_a_record_with_nothing_to_fill() {
    let fixture = Fixture::with_vault();
    let key = fixture.directory().join("id_test");
    std::fs::write(&key, "-----BEGIN TEST KEY-----\n").unwrap();
    fixture
        .sefy()
        .args(["add", "ssh-key", "box", "--no-passphrase", "--private-key"])
        .arg(&key)
        .assert()
        .success();

    fixture
        .sefy()
        .args(["fill", "box"])
        .assert()
        .failure()
        .stderr(contains("holds nothing a ssh-key fills in"))
        .stderr(contains("private-key"));
}

#[test]
fn fill_on_a_note_is_refused() {
    let fixture = Fixture::with_vault();
    add_note(&fixture, "diary", "dear diary", &[]);
    fixture
        .sefy()
        .args(["fill", "diary"])
        .assert()
        .failure()
        .stderr(contains("is a note"));
}

// ---------------------------------------------------------------------------
// Kinds that are nothing but their template

#[test]
fn a_wifi_network_takes_its_public_fields_on_the_line_and_its_key_from_aside() {
    let fixture = Fixture::with_vault();
    fixture
        .sefy()
        .env("WIFI_KEY", "correct horse")
        .args([
            "add",
            "wifi",
            "home",
            "--set",
            "security=WPA3",
            "--set",
            "ssid=HomeNet",
        ])
        .args(["--secret-env", "password=WIFI_KEY", "--tag", "house"])
        .assert()
        .success()
        .stdout(contains("added \"home\""));

    // The template's order, not the order of the options.
    let shown = fixture
        .sefy()
        .args(["show", "home"])
        .assert()
        .success()
        .stdout(contains("correct horse").not())
        .get_output()
        .stdout
        .clone();
    let shown = String::from_utf8(shown).unwrap();
    let ssid = shown.find("ssid:").unwrap();
    let password = shown.find("password:").unwrap();
    let security = shown.find("security:").unwrap();
    assert!(ssid < password && password < security, "{shown}");
    assert!(shown.contains("kind:        wifi"), "{shown}");

    // The default field is the network key.
    fixture
        .sefy()
        .args(["get", "home", "--stdout"])
        .assert()
        .success()
        .stdout("correct horse\n");

    fixture
        .sefy()
        .args(["ls", "--kind", "wifi"])
        .assert()
        .success()
        .stdout(contains("home"));
}

#[test]
fn a_secret_field_is_never_taken_on_the_command_line() {
    let fixture = Fixture::with_vault();
    fixture
        .sefy()
        .args([
            "add",
            "wifi",
            "home",
            "--set",
            "password=in the history now",
        ])
        .assert()
        .failure()
        .stderr(contains("password is secret"))
        .stderr(contains("--secret-env password=VAR"));
}

#[test]
fn a_secret_field_left_to_the_prompt_needs_a_terminal() {
    // Proof that the key is asked for: with no terminal and no variable, the
    // prompt is what fails.
    let fixture = Fixture::with_vault();
    fixture
        .sefy()
        .args([
            "add",
            "api-token",
            "ci",
            "--set",
            "url=https://example.invalid",
        ])
        .assert()
        .failure()
        .stderr(contains("input is not a terminal"));
}

#[test]
fn skip_leaves_a_secret_out_and_names_only_secret_fields() {
    let fixture = Fixture::with_vault();
    fixture
        .sefy()
        .args(["add", "api-token", "ci", "--skip", "token"])
        .args([
            "--set",
            "url=https://example.invalid",
            "--set",
            "scopes=read",
        ])
        .assert()
        .success();
    fixture
        .sefy()
        .args(["show", "ci"])
        .assert()
        .success()
        .stdout(contains("token:").not())
        .stdout(contains("scopes:"));

    fixture
        .sefy()
        .args(["add", "api-token", "other", "--skip", "url"])
        .assert()
        .failure()
        .stderr(contains("those are: token"));
}

#[test]
fn a_bank_account_keeps_extra_fields_and_leaves_an_empty_secret_out() {
    let fixture = Fixture::with_vault();
    fixture
        .sefy()
        .env("ACCOUNT", "DE00 1234 5678")
        .env("EMPTY", "")
        .args(["add", "bank", "savings", "--secret-env", "account=ACCOUNT"])
        .args(["--set", "holder=Someone", "--set", "branch=Main street"])
        .args(["--secret-env", "online-pin=EMPTY"])
        .assert()
        .success();

    fixture
        .sefy()
        .args(["show", "savings"])
        .assert()
        .success()
        .stdout(contains("branch:"))
        .stdout(contains("Main street"))
        .stdout(contains("online-pin").not())
        .stdout(contains("1234").not());
    fixture
        .sefy()
        .args(["get", "savings", "--stdout"])
        .assert()
        .success()
        .stdout("DE00 1234 5678\n");
}

#[test]
fn a_name_given_twice_is_refused_rather_than_guessed() {
    let fixture = Fixture::with_vault();
    fixture
        .sefy()
        .args(["add", "wifi", "home", "--skip", "password"])
        .args(["--set", "ssid=one", "--set", "ssid=two"])
        .assert()
        .failure()
        .stderr(contains("names \"ssid\" twice"));
}

#[test]
fn the_new_kinds_survive_an_export_and_an_import() {
    let source = Fixture::with_vault();
    source
        .sefy()
        .env("WIFI_KEY", "correct horse")
        .args(["add", "wifi", "home", "--set", "ssid=HomeNet"])
        .args(["--secret-env", "password=WIFI_KEY"])
        .assert()
        .success();
    let export = source.directory().join("export.json");
    source
        .sefy()
        .args(["export", "--i-know-this-writes-plaintext", "--output"])
        .arg(&export)
        .assert()
        .success();

    let target = Fixture::with_vault();
    target
        .sefy()
        .arg("import")
        .arg(&export)
        .assert()
        .success()
        .stdout(contains("imported 1 item"));
    target
        .sefy()
        .args(["get", "home", "--stdout"])
        .assert()
        .success()
        .stdout("correct horse\n");
}
