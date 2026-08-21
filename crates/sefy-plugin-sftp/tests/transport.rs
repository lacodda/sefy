//! The transport against stand-in `ssh` and `scp` programs.
//!
//! What this plugin does is build two command lines and interpret what comes
//! back, so that is what these tests exercise: the stand-ins record their
//! arguments and act on a local directory playing the server. A real server is
//! checked by hand before a release — the CI runners have none, and a test that
//! skipped itself without one would report "ok" for a transport that never ran.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// A directory placed at the front of PATH, holding stand-in ssh and scp.
///
/// `server` is the directory they treat as the far side; `log` collects one
/// line per invocation, so a test can assert on what was actually run.
struct Fake {
    directory: tempfile::TempDir,
    server: PathBuf,
    log: PathBuf,
}

impl Fake {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let server = directory.path().join("server");
        std::fs::create_dir_all(&server).unwrap();
        let log = directory.path().join("commands.log");
        std::fs::write(&log, "").unwrap();

        // Copied rather than built twice: the stand-in decides which program it
        // is playing from its own file name.
        for program in ["ssh", "scp"] {
            std::fs::copy(
                stand_in(),
                directory
                    .path()
                    .join(format!("{program}{}", std::env::consts::EXE_SUFFIX)),
            )
            .unwrap();
        }

        Self {
            directory,
            server,
            log,
        }
    }

    /// Runs the plugin with these stand-ins ahead of the real programs.
    fn run(&self, request: &str, destination: &str) -> (bool, String, String) {
        use std::io::Write;

        let mut child = Command::new(plugin_binary())
            .arg("run")
            .env("PATH", self.path_with_fakes())
            .env("SEFY_SFTP_DESTINATION", destination)
            .env("SEFY_FAKE_SERVER", &self.server)
            .env("SEFY_FAKE_LOG", &self.log)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("the plugin binary must be runnable");

        child
            .stdin
            .take()
            .unwrap()
            .write_all(request.as_bytes())
            .unwrap();

        let output = child.wait_with_output().unwrap();
        (
            output.status.success(),
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
    }

    /// PATH with the stand-ins first, and the system directories still behind
    /// them — the plugin is what is under test, not the environment.
    fn path_with_fakes(&self) -> std::ffi::OsString {
        let mut path = self.directory.path().as_os_str().to_owned();
        path.push(if cfg!(windows) { ";" } else { ":" });
        path.push(std::env::var_os("PATH").unwrap_or_default());
        path
    }

    /// Every command line the stand-ins were given, in order.
    fn commands(&self) -> Vec<String> {
        std::fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(str::to_owned)
            .collect()
    }

    fn server_file(&self, name: &str) -> PathBuf {
        self.server.join(name)
    }
}

/// Builds the stand-in program once per test run and returns its path.
///
/// A shell script was tried first and does not work: Rust on Windows resolves
/// `Command::new("scp")` to an executable only, never to a `.cmd` on PATH — so
/// a stand-in that is not a real program is simply never called, and the tests
/// silently exercise the machine's actual ssh instead.
fn stand_in() -> &'static Path {
    static BUILT: OnceLock<PathBuf> = OnceLock::new();

    BUILT.get_or_init(|| {
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixture_stand_in.rs.txt");

        // Beside the test binary, so it is rebuilt whenever the target
        // directory is cleaned and never lands in the packaged crate.
        let mut output = std::env::current_exe().unwrap();
        output.pop();
        let output = output.join(format!("sefy-stand-in{}", std::env::consts::EXE_SUFFIX));

        let status = Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
            .arg("--edition=2024")
            .arg("-O")
            // Named explicitly: rustc would otherwise derive the crate name
            // from the file name, and the extension that keeps cargo from
            // treating this file as a test target is not valid in one.
            .arg("--crate-name=sefy_stand_in")
            .arg(&source)
            .arg("-o")
            .arg(&output)
            .status()
            .expect("rustc must be available: it is what built these tests");
        assert!(status.success(), "the stand-in program did not compile");

        output
    })
}

/// Path of the built plugin, beside the test binary.
fn plugin_binary() -> PathBuf {
    let mut path = std::env::current_exe().unwrap();
    path.pop();
    if path.ends_with("deps") {
        path.pop();
    }
    path.join(format!("sefy-plugin-sftp{}", std::env::consts::EXE_SUFFIX))
}

fn request(operation: &str, file: &Path, name: &str) -> String {
    format!(
        r#"{{"operation":"{operation}","file":{},"name":"{name}"}}"#,
        serde_json::to_string(&file.display().to_string()).unwrap()
    )
}

const DESTINATION: &str = "you@server.example:/srv/vaults";

#[test]
fn the_manifest_is_what_sefy_expects() {
    let output = Command::new(plugin_binary())
        .arg("--manifest")
        .output()
        .unwrap();

    assert!(output.status.success());
    let manifest: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(manifest["protocol_version"], 1);
    assert_eq!(manifest["name"], "sftp");
}

#[test]
fn a_vault_pushed_here_comes_back_byte_for_byte() {
    let fake = Fake::new();
    let vault = fake.directory.path().join("notes.bak");
    std::fs::write(&vault, b"sealed bytes, not a real vault").unwrap();

    let (ok, stdout, stderr) = fake.run(&request("push", &vault, "vault"), DESTINATION);
    assert!(ok, "push failed: {stderr}");
    assert!(stdout.contains("pushed"), "got: {stdout}");

    let fetched = fake.directory.path().join("fetched.bin");
    let (ok, stdout, stderr) = fake.run(&request("pull", &fetched, "vault"), DESTINATION);
    assert!(ok, "pull failed: {stderr}");
    assert!(stdout.contains("fetched"), "got: {stdout}");

    assert_eq!(
        std::fs::read(&fetched).unwrap(),
        b"sealed bytes, not a real vault"
    );
}

#[test]
fn a_push_lands_under_a_staging_name_before_it_replaces_anything() {
    // scp writes straight into its destination, so uploading over the live
    // file would leave a truncated blob if the transfer broke — and a
    // truncated vault does not open at all. The bytes go somewhere else first
    // and are moved into place.
    let fake = Fake::new();
    let vault = fake.directory.path().join("notes.bak");
    std::fs::write(&vault, b"sealed").unwrap();

    let (ok, _, stderr) = fake.run(&request("push", &vault, "vault"), DESTINATION);
    assert!(ok, "{stderr}");

    let commands = fake.commands();
    let upload = commands
        .iter()
        .find(|line| line.starts_with("scp "))
        .expect("an upload must have happened");
    assert!(
        upload.contains("vault.incoming"),
        "the upload must not write over the live copy: {upload}"
    );
    assert!(
        commands.iter().any(|line| line.contains("mv --")),
        "the staged file must be moved into place: {commands:?}"
    );
    assert!(
        fake.server_file("vault").is_file(),
        "the vault must end up under its own name"
    );
    assert!(
        !fake.server_file("vault.incoming").exists(),
        "the staging file must not be left behind"
    );
}

#[test]
fn a_second_push_replaces_what_the_server_holds() {
    let fake = Fake::new();
    let vault = fake.directory.path().join("notes.bak");

    std::fs::write(&vault, b"first").unwrap();
    assert!(fake.run(&request("push", &vault, "vault"), DESTINATION).0);

    std::fs::write(&vault, b"second").unwrap();
    assert!(fake.run(&request("push", &vault, "vault"), DESTINATION).0);

    assert_eq!(std::fs::read(fake.server_file("vault")).unwrap(), b"second");
}

#[test]
fn two_names_are_two_files_on_the_server() {
    let fake = Fake::new();
    let vault = fake.directory.path().join("notes.bak");

    std::fs::write(&vault, b"desktop copy").unwrap();
    assert!(fake.run(&request("push", &vault, "desktop"), DESTINATION).0);
    std::fs::write(&vault, b"laptop copy").unwrap();
    assert!(fake.run(&request("push", &vault, "laptop"), DESTINATION).0);

    assert_eq!(
        std::fs::read(fake.server_file("desktop")).unwrap(),
        b"desktop copy"
    );
    assert_eq!(
        std::fs::read(fake.server_file("laptop")).unwrap(),
        b"laptop copy"
    );
}

#[test]
fn pulling_something_the_server_does_not_hold_says_which_name() {
    let fake = Fake::new();
    let fetched = fake.directory.path().join("fetched.bin");

    let (ok, _, stderr) = fake.run(&request("pull", &fetched, "never-pushed"), DESTINATION);

    assert!(!ok);
    assert!(stderr.contains("never-pushed"), "got: {stderr}");
    assert!(
        !fetched.exists(),
        "nothing must be written when there is nothing to fetch"
    );
}

#[test]
fn without_the_destination_variable_the_error_says_what_to_set() {
    let fake = Fake::new();
    let vault = fake.directory.path().join("notes.bak");
    std::fs::write(&vault, b"sealed").unwrap();

    let (ok, _, stderr) = fake.run(&request("push", &vault, "vault"), "");

    assert!(!ok);
    assert!(stderr.contains("SEFY_SFTP_DESTINATION"), "got: {stderr}");
}

#[test]
fn a_destination_without_a_directory_is_refused_before_anything_runs() {
    let fake = Fake::new();
    let vault = fake.directory.path().join("notes.bak");
    std::fs::write(&vault, b"sealed").unwrap();

    let (ok, _, stderr) = fake.run(&request("push", &vault, "vault"), "server.example");

    assert!(!ok);
    assert!(stderr.contains("host:/path"), "got: {stderr}");
    assert!(
        fake.commands().is_empty(),
        "nothing should have been run: {:?}",
        fake.commands()
    );
}

#[test]
fn a_remote_name_that_would_need_quoting_never_reaches_the_server() {
    // The remote side runs `mv` and `test` through a shell. A name is refused
    // rather than escaped, and refused *before* any command is built.
    let fake = Fake::new();
    let vault = fake.directory.path().join("notes.bak");
    std::fs::write(&vault, b"sealed").unwrap();

    let (ok, _, stderr) = fake.run(
        &request("push", &vault, "vault; rm -rf /tmp/whatever"),
        DESTINATION,
    );

    assert!(!ok);
    assert!(stderr.contains("--name"), "got: {stderr}");
    assert!(
        fake.commands().is_empty(),
        "nothing should have been run: {:?}",
        fake.commands()
    );
}

#[test]
fn every_call_tells_ssh_not_to_ask_for_anything() {
    // A plugin runs without a terminal. Without BatchMode a server that wants
    // a password would leave the sync hanging with nothing to say why.
    let fake = Fake::new();
    let vault = fake.directory.path().join("notes.bak");
    std::fs::write(&vault, b"sealed").unwrap();

    assert!(fake.run(&request("push", &vault, "vault"), DESTINATION).0);

    let commands = fake.commands();
    assert!(!commands.is_empty());
    for line in &commands {
        assert!(
            line.contains("BatchMode=yes"),
            "every call must refuse to prompt: {line}"
        );
    }
}

#[test]
fn run_by_hand_it_explains_what_it_is_instead_of_waiting_for_input() {
    let output = Command::new(plugin_binary()).output().unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("sefy push"), "got: {stderr}");
    assert!(stderr.contains("SEFY_SFTP_DESTINATION"), "got: {stderr}");
}
