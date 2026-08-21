//! The transport against a real git repository.
//!
//! The "remote" is a bare repository in a temporary directory, which is a real
//! remote as far as git is concerned — no network, and nothing about the test
//! that a GitHub URL would exercise differently.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Runs the plugin with a request on stdin, returning (success, stdout, stderr).
fn run_plugin(request: &str, repository: &Path, working_copy: &Path) -> (bool, String, String) {
    use std::io::Write;

    let mut child = Command::new(plugin_binary())
        .arg("run")
        .env("SEFY_GITHUB_REPO", repository)
        .env("SEFY_GITHUB_DIR", working_copy)
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

/// Path of the built plugin, beside the test binary.
fn plugin_binary() -> PathBuf {
    let mut path = std::env::current_exe().unwrap();
    path.pop();
    if path.ends_with("deps") {
        path.pop();
    }
    path.join(format!(
        "sefy-plugin-github{}",
        std::env::consts::EXE_SUFFIX
    ))
}

fn git(directory: Option<&Path>, arguments: &[&str]) -> String {
    let mut command = Command::new("git");
    if let Some(directory) = directory {
        command.current_dir(directory);
    }
    let output = command.args(arguments).output().unwrap();
    assert!(
        output.status.success(),
        "git {arguments:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// A bare repository with one commit, so it has a branch to clone.
///
/// A freshly initialised bare repository has no HEAD to check out, and cloning
/// it produces a working copy git considers to be on an unborn branch — which
/// is not the shape this transport is written against, and not what anyone
/// pushing a vault to a repository they already use would have.
fn remote(directory: &Path) -> PathBuf {
    let remote = directory.join("remote.git");
    git(
        None,
        &["init", "--quiet", "--bare", &remote.display().to_string()],
    );

    let seed = directory.join("seed");
    git(
        None,
        &[
            "clone",
            "--quiet",
            &remote.display().to_string(),
            &seed.display().to_string(),
        ],
    );
    std::fs::write(seed.join("README"), "vault storage\n").unwrap();
    git(Some(&seed), &["add", "README"]);
    git(
        Some(&seed),
        &[
            "-c",
            "user.email=t@example.invalid",
            "-c",
            "user.name=Test",
            "commit",
            "--quiet",
            "-m",
            "first",
        ],
    );
    git(Some(&seed), &["push", "--quiet", "origin", "HEAD"]);

    remote
}

fn request(operation: &str, file: &Path, name: &str) -> String {
    format!(
        r#"{{"operation":"{operation}","file":{},"name":"{name}"}}"#,
        serde_json::to_string(&file.display().to_string()).unwrap()
    )
}

#[test]
fn the_manifest_is_what_sefy_expects() {
    let output = Command::new(plugin_binary())
        .arg("--manifest")
        .output()
        .unwrap();

    assert!(output.status.success());
    let manifest: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(manifest["protocol_version"], 1);
    assert_eq!(manifest["name"], "github");
}

#[test]
fn a_vault_pushed_here_comes_back_from_a_second_machine() {
    let directory = tempfile::tempdir().unwrap();
    let remote = remote(directory.path());

    // One machine pushes.
    let vault = directory.path().join("notes.bak");
    std::fs::write(&vault, b"sealed bytes, not a real vault").unwrap();
    let (ok, stdout, stderr) = run_plugin(
        &request("push", &vault, "vault"),
        &remote,
        &directory.path().join("machine-one"),
    );
    assert!(ok, "push failed: {stderr}");
    assert!(stdout.contains("pushed"), "got: {stdout}");

    // Another machine, with its own working copy, pulls.
    let fetched = directory.path().join("fetched.bin");
    let (ok, stdout, stderr) = run_plugin(
        &request("pull", &fetched, "vault"),
        &remote,
        &directory.path().join("machine-two"),
    );
    assert!(ok, "pull failed: {stderr}");
    assert!(stdout.contains("fetched"), "got: {stdout}");

    assert_eq!(
        std::fs::read(&fetched).unwrap(),
        b"sealed bytes, not a real vault",
        "what comes back is what went up, byte for byte"
    );
}

#[test]
fn a_second_push_from_the_same_machine_updates_the_remote() {
    let directory = tempfile::tempdir().unwrap();
    let remote = remote(directory.path());
    let working_copy = directory.path().join("machine");
    let vault = directory.path().join("notes.bak");

    std::fs::write(&vault, b"first").unwrap();
    let (ok, _, stderr) = run_plugin(&request("push", &vault, "vault"), &remote, &working_copy);
    assert!(ok, "first push failed: {stderr}");

    std::fs::write(&vault, b"second").unwrap();
    let (ok, _, stderr) = run_plugin(&request("push", &vault, "vault"), &remote, &working_copy);
    assert!(ok, "second push failed: {stderr}");

    let fetched = directory.path().join("fetched.bin");
    let (ok, _, stderr) = run_plugin(
        &request("pull", &fetched, "vault"),
        &remote,
        &directory.path().join("elsewhere"),
    );
    assert!(ok, "pull failed: {stderr}");
    assert_eq!(std::fs::read(&fetched).unwrap(), b"second");
}

#[test]
fn a_push_of_what_is_already_there_says_so_without_committing() {
    let directory = tempfile::tempdir().unwrap();
    let remote = remote(directory.path());
    let working_copy = directory.path().join("machine");
    let vault = directory.path().join("notes.bak");
    std::fs::write(&vault, b"unchanged").unwrap();

    run_plugin(&request("push", &vault, "vault"), &remote, &working_copy);
    let before = git(Some(&working_copy), &["rev-parse", "HEAD"]);

    let (ok, stdout, stderr) =
        run_plugin(&request("push", &vault, "vault"), &remote, &working_copy);

    assert!(ok, "push failed: {stderr}");
    assert!(stdout.contains("already"), "got: {stdout}");
    assert_eq!(
        git(Some(&working_copy), &["rev-parse", "HEAD"]),
        before,
        "an unchanged vault must not add a commit"
    );
}

#[test]
fn two_machines_pushing_different_names_do_not_tread_on_each_other() {
    let directory = tempfile::tempdir().unwrap();
    let remote = remote(directory.path());

    let one = directory.path().join("one.bak");
    std::fs::write(&one, b"machine one").unwrap();
    let (ok, _, stderr) = run_plugin(
        &request("push", &one, "desktop"),
        &remote,
        &directory.path().join("machine-one"),
    );
    assert!(ok, "{stderr}");

    let two = directory.path().join("two.bak");
    std::fs::write(&two, b"machine two").unwrap();
    let (ok, _, stderr) = run_plugin(
        &request("push", &two, "laptop"),
        &remote,
        &directory.path().join("machine-two"),
    );
    assert!(ok, "{stderr}");

    // The second push must not have dropped the first machine's copy: a
    // transport that reset over the remote's state would.
    let fetched = directory.path().join("fetched.bin");
    let (ok, _, stderr) = run_plugin(
        &request("pull", &fetched, "desktop"),
        &remote,
        &directory.path().join("machine-three"),
    );
    assert!(ok, "the first copy must still be there: {stderr}");
    assert_eq!(std::fs::read(&fetched).unwrap(), b"machine one");
}

#[test]
fn pulling_something_the_repository_does_not_hold_says_which_name() {
    let directory = tempfile::tempdir().unwrap();
    let remote = remote(directory.path());
    let fetched = directory.path().join("fetched.bin");

    let (ok, _, stderr) = run_plugin(
        &request("pull", &fetched, "never-pushed"),
        &remote,
        &directory.path().join("machine"),
    );

    assert!(!ok);
    assert!(stderr.contains("never-pushed"), "got: {stderr}");
    assert!(
        !fetched.exists(),
        "nothing must be written when there is nothing to fetch"
    );
}

#[test]
fn without_the_repository_variable_the_error_says_what_to_set() {
    let directory = tempfile::tempdir().unwrap();
    let vault = directory.path().join("notes.bak");
    std::fs::write(&vault, b"sealed").unwrap();

    let mut child = Command::new(plugin_binary())
        .arg("run")
        .env_remove("SEFY_GITHUB_REPO")
        .env("SEFY_GITHUB_DIR", directory.path().join("machine"))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    {
        use std::io::Write;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(request("push", &vault, "vault").as_bytes())
            .unwrap();
    }
    let output = child.wait_with_output().unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("SEFY_GITHUB_REPO"), "got: {stderr}");
}

#[test]
fn run_by_hand_it_explains_what_it_is_instead_of_waiting_for_input() {
    let output = Command::new(plugin_binary()).output().unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("sefy push"), "got: {stderr}");
}

#[test]
fn a_push_landing_on_top_of_someone_elses_takes_the_remotes_state_first() {
    let directory = tempfile::tempdir().unwrap();
    let remote = remote(directory.path());

    // One machine publishes, so the remote has a copy nobody else has seen.
    let theirs = directory.path().join("theirs.bak");
    std::fs::write(&theirs, b"from the other machine").unwrap();
    let (ok, _, stderr) = run_plugin(
        &request("push", &theirs, "vault"),
        &remote,
        &directory.path().join("machine-one"),
    );
    assert!(ok, "{stderr}");

    // A second machine that has never pulled pushes its own. This is the case
    // that would fail outright without the fetch-and-reset in prepare: git
    // would refuse a push whose history the remote has moved past.
    let ours = directory.path().join("ours.bak");
    std::fs::write(&ours, b"from this machine").unwrap();
    let (ok, _, stderr) = run_plugin(
        &request("push", &ours, "vault"),
        &remote,
        &directory.path().join("machine-two"),
    );

    assert!(
        ok,
        "the push must go through, not stall on a diverged history: {stderr}"
    );

    // The push wins outright — which is exactly why sefy pulls before it
    // pushes, and why `sefy sync` exists rather than a bare `push`.
    let fetched = directory.path().join("fetched.bin");
    let (ok, _, stderr) = run_plugin(
        &request("pull", &fetched, "vault"),
        &remote,
        &directory.path().join("machine-three"),
    );
    assert!(ok, "{stderr}");
    assert_eq!(std::fs::read(&fetched).unwrap(), b"from this machine");
}

#[test]
fn a_stale_working_copy_catches_up_before_it_pushes() {
    let directory = tempfile::tempdir().unwrap();
    let remote = remote(directory.path());
    let ours = directory.path().join("machine-one");
    let theirs = directory.path().join("machine-two");

    // This machine pushes once, so it now has a working copy of its own.
    let vault = directory.path().join("notes.bak");
    std::fs::write(&vault, b"first").unwrap();
    let (ok, _, stderr) = run_plugin(&request("push", &vault, "vault"), &remote, &ours);
    assert!(ok, "{stderr}");

    // Another machine publishes twice, moving the remote two commits ahead of
    // what this machine's working copy knows about.
    let other = directory.path().join("other.bak");
    for contents in [b"theirs one".as_slice(), b"theirs two".as_slice()] {
        std::fs::write(&other, contents).unwrap();
        let (ok, _, stderr) = run_plugin(&request("push", &other, "vault"), &remote, &theirs);
        assert!(ok, "{stderr}");
    }

    // Now this machine pushes again from its stale copy. Without catching up
    // first, git refuses the push: the remote has moved past this history.
    std::fs::write(&vault, b"second").unwrap();
    let (ok, _, stderr) = run_plugin(&request("push", &vault, "vault"), &remote, &ours);

    assert!(
        ok,
        "a stale working copy must catch up rather than stall: {stderr}"
    );

    let fetched = directory.path().join("fetched.bin");
    let (ok, _, stderr) = run_plugin(
        &request("pull", &fetched, "vault"),
        &remote,
        &directory.path().join("machine-three"),
    );
    assert!(ok, "{stderr}");
    assert_eq!(std::fs::read(&fetched).unwrap(), b"second");
}

#[test]
fn a_changed_vault_of_the_same_length_is_still_published() {
    let directory = tempfile::tempdir().unwrap();
    let remote = remote(directory.path());
    let working_copy = directory.path().join("machine");
    let vault = directory.path().join("notes.bak");

    // The shape that caught this out in practice. A sealed vault is the same
    // length every time it is re-sealed after an ordinary edit, and the vault
    // handed over is a file written before the previous push — so the copy
    // landing in the working copy can carry an mtime *older* than the one git
    // recorded in its index. git then reports the path as unchanged, and a
    // transport that believes it publishes the previous contents, silently,
    // with a success message.
    //
    // The older vault is prepared first so its timestamp is genuinely behind.
    let older = directory.path().join("older.bak");
    let second = vec![b'B'; 53_305];
    std::fs::write(&older, &second).unwrap();

    let first = vec![b'A'; 53_305];
    std::fs::write(&vault, &first).unwrap();
    let (ok, _, stderr) = run_plugin(&request("push", &vault, "vault"), &remote, &working_copy);
    assert!(ok, "{stderr}");

    // fs::copy carries the source's timestamps across on Windows, which is how
    // the working copy ends up looking older than the index believes it to be.
    std::fs::copy(&older, &vault).unwrap();
    let (ok, stdout, stderr) =
        run_plugin(&request("push", &vault, "vault"), &remote, &working_copy);
    assert!(ok, "{stderr}");
    assert!(
        stdout.contains("pushed"),
        "the second push must not report the vault as unchanged: {stdout}"
    );

    let fetched = directory.path().join("fetched.bin");
    let (ok, _, stderr) = run_plugin(
        &request("pull", &fetched, "vault"),
        &remote,
        &directory.path().join("elsewhere"),
    );
    assert!(ok, "{stderr}");
    assert_eq!(
        std::fs::read(&fetched).unwrap(),
        second,
        "what the remote holds must be the second vault, not the first"
    );
}

#[test]
fn a_push_works_on_a_machine_with_no_git_identity_configured() {
    // git refuses to commit without user.name and user.email, and the machine
    // running this may well have neither — a fresh install, a CI runner. The
    // transport supplies its own rather than failing the sync with git's "tell
    // me who you are", which is a puzzling thing to meet when all you asked was
    // to move a vault.
    let directory = tempfile::tempdir().unwrap();
    let remote = remote(directory.path());
    let vault = directory.path().join("notes.bak");
    std::fs::write(&vault, b"sealed").unwrap();

    // An empty HOME (and the Windows equivalents) means no global config to
    // read an identity from, whatever the real machine has set.
    let empty_home = directory.path().join("no-config");
    std::fs::create_dir_all(&empty_home).unwrap();

    let mut child = Command::new(plugin_binary())
        .arg("run")
        .env("SEFY_GITHUB_REPO", &remote)
        .env("SEFY_GITHUB_DIR", directory.path().join("machine"))
        .env("HOME", &empty_home)
        .env("USERPROFILE", &empty_home)
        .env("GIT_CONFIG_GLOBAL", empty_home.join("gitconfig"))
        .env("GIT_CONFIG_SYSTEM", empty_home.join("gitconfig"))
        .env_remove("GIT_AUTHOR_NAME")
        .env_remove("GIT_AUTHOR_EMAIL")
        .env_remove("GIT_COMMITTER_NAME")
        .env_remove("GIT_COMMITTER_EMAIL")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    {
        use std::io::Write;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(request("push", &vault, "vault").as_bytes())
            .unwrap();
    }
    let output = child.wait_with_output().unwrap();

    assert!(
        output.status.success(),
        "the push must not need an identity: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let fetched = directory.path().join("fetched.bin");
    let (ok, _, stderr) = run_plugin(
        &request("pull", &fetched, "vault"),
        &remote,
        &directory.path().join("elsewhere"),
    );
    assert!(ok, "{stderr}");
    assert_eq!(std::fs::read(&fetched).unwrap(), b"sealed");
}
