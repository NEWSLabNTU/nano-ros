//! Dispatch as it actually happens — through the real `nros` binary
//! (phase-440 W7, RFC-0095 D8).
//!
//! `nros-cli-core/tests/toolchain_pin_dispatch.rs` covers what the launcher
//! DECIDES. This file covers whether the decision reaches an `exec`, and it is
//! the only place that can answer the property D8 actually states:
//!
//! > it must keep working when it is OLDER than what it launches.
//!
//! The same argument `store_verbs.rs` makes one wave back — twelve tests passed
//! while `nros toolchain uninstall` could not be invoked at all, because each of
//! them constructed the `Args` and called `run`. A launcher that cannot exec is
//! not a launcher.
//!
//! Shape: the REAL binary is installed as the launcher at
//! `<store>/sdk/nros/0.5.0-nros1/bin/nros`, and a shell script stands in for the
//! NEWER toolchain at `<store>/sdk/nros/0.6.0-nros1/bin/nros`. A script is the
//! right stand-in and not a shortcut: what is being tested is that the launcher
//! hands over to whatever is at the pinned path, and a script can PROVE what it
//! received in a way a second copy of the same binary cannot.
//!
//! Cheap by construction: no fixtures, no provisioning, no network, and the
//! store is a tempdir named by `$NROS_STORE`.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

/// Install the real binary as the launcher for `version`.
fn install_launcher(store: &Path, version: &str) -> PathBuf {
    let bin = store.join("sdk/nros").join(version).join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let exe = bin.join("nros");
    // NOT `fs::copy` — issue 0476. This binary is EXEC'd a few lines later, and
    // a write descriptor held in this process is inherited by any sibling
    // thread's fork until that child execs, which the kernel reports as
    // ETXTBSY. It passes on an idle machine and fails on a loaded CI runner.
    nros_cli_core::test_support::copy_executable(
        std::path::Path::new(env!("CARGO_BIN_EXE_nros")),
        &exe,
    );
    exe
}

/// A stand-in toolchain that reports what it was handed: its own name, then one
/// line per argument. Written as a script so the assertions can be about the
/// HANDOVER rather than about a second binary's behaviour.
fn install_stub(store: &Path, version: &str) -> PathBuf {
    let bin = store.join("sdk/nros").join(version).join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let exe = bin.join("nros");
    // Same rule as `install_launcher`, and this is the shape issue 0476 was
    // filed against: written here, exec'd there.
    nros_cli_core::test_support::write_executable_stub(
        &exe,
        "#!/bin/sh\n\
         echo \"STUB-TOOLCHAIN\"\n\
         echo \"DISPATCH=${NROS_TOOLCHAIN_DISPATCH:-unset}\"\n\
         for a in \"$@\"; do echo \"ARG=$a\"; done\n",
    );
    exe
}

fn pin(project: &Path, version: &str) {
    std::fs::create_dir_all(project).unwrap();
    std::fs::write(
        project.join("nros-toolchain.toml"),
        format!("[toolchain]\nversion = \"{version}\"\n"),
    )
    .unwrap();
}

/// Run `launcher` from `cwd` against `store`, with a clean-enough environment
/// that no developer's real `~/.nros` or `$NROS_HOME` can decide the answer.
fn run(launcher: &Path, cwd: &Path, store: &Path, args: &[&str]) -> (bool, String) {
    let out = Command::new(launcher)
        .args(args)
        .current_dir(cwd)
        .env("NROS_STORE", store)
        .env_remove("NROS_HOME")
        .env_remove("NROS_TOOLCHAIN_DISPATCH")
        .env_remove("NROS_SKIP_TOOLCHAIN_DISPATCH")
        .output()
        .expect("failed to exec the launcher");
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.success(), text)
}

/// D8, end to end, through a real `exec`.
///
/// The argument is deliberately one the launcher's OWN clap would reject
/// (`--a-flag-this-binary-does-not-have`). If dispatch happened after parsing,
/// this invocation could not reach the stub at all — so a passing test is
/// evidence for the ordering the module header claims, not only for the
/// handover. It also proves `argv` is forwarded verbatim and that the child is
/// marked, which is what stops an exec loop.
#[test]
fn an_older_launcher_execs_the_newer_pinned_toolchain() {
    let tmp = tempfile::tempdir().unwrap();
    let store = tmp.path().join("store");
    let launcher = install_launcher(&store, "0.5.0-nros1");
    install_stub(&store, "0.6.0-nros1");
    let proj = tmp.path().join("my-robot");
    pin(&proj, "0.6.0-nros1");

    let (ok, text) = run(
        &launcher,
        &proj,
        &store,
        &["--a-flag-this-binary-does-not-have"],
    );
    assert!(ok, "the launcher did not hand over cleanly:\n{text}");
    assert!(
        text.contains("STUB-TOOLCHAIN"),
        "the pinned toolchain never ran:\n{text}"
    );
    assert!(
        text.contains("DISPATCH=0.6.0-nros1"),
        "the child was not marked as dispatched, so it could exec again:\n{text}"
    );
    assert!(
        text.contains("ARG=--a-flag-this-binary-does-not-have"),
        "argv was not forwarded verbatim — so the decision was made after \
         parsing, which is what D8 forbids:\n{text}"
    );
}

/// The pin is found from a SUBDIRECTORY here too, through the real binary: a
/// user runs `nros build` from inside a package as often as from the root.
#[test]
fn the_launcher_finds_the_pin_from_a_subdirectory() {
    let tmp = tempfile::tempdir().unwrap();
    let store = tmp.path().join("store");
    let launcher = install_launcher(&store, "0.5.0-nros1");
    install_stub(&store, "0.6.0-nros1");
    let proj = tmp.path().join("my-robot");
    pin(&proj, "0.6.0-nros1");
    let deep = proj.join("src/talker_pkg/src");
    std::fs::create_dir_all(&deep).unwrap();

    let (ok, text) = run(&launcher, &deep, &store, &["--version"]);
    assert!(ok, "{text}");
    assert!(
        text.contains("STUB-TOOLCHAIN"),
        "a build from a subdirectory did not reach the pinned toolchain:\n{text}"
    );
}

/// An unpinned project runs in the launcher itself — the version it prints is
/// the launcher's own, and no stub is reached even though one is installed.
///
/// The negative control for the test above: without it, a stub that never ran
/// and a launcher that always dispatches would be indistinguishable.
#[test]
fn an_unpinned_project_runs_in_the_launcher() {
    let tmp = tempfile::tempdir().unwrap();
    let store = tmp.path().join("store");
    let launcher = install_launcher(&store, "0.5.0-nros1");
    install_stub(&store, "0.6.0-nros1");
    let proj = tmp.path().join("my-robot");
    std::fs::create_dir_all(&proj).unwrap();

    let (ok, text) = run(&launcher, &proj, &store, &["--version"]);
    assert!(ok, "{text}");
    assert!(
        !text.contains("STUB-TOOLCHAIN"),
        "an unpinned project must not be dispatched anywhere:\n{text}"
    );
    assert!(text.contains("nros"), "expected a version line:\n{text}");
}

/// A pin the store cannot answer must REFUSE, naming the version and the pin
/// file — never proceed silently in whatever binary happens to be fronted,
/// which would build the project with a toolchain nobody chose.
///
/// This launcher carries no bundled installer (the copy above brings no
/// `share/nros/install.sh`), so it is the refusal arm rather than the fetch
/// arm — which is exactly the state an asset built before phase-440 W7 is in.
#[test]
fn a_pin_naming_an_absent_toolchain_refuses_and_names_it() {
    let tmp = tempfile::tempdir().unwrap();
    let store = tmp.path().join("store");
    let launcher = install_launcher(&store, "0.5.0-nros1");
    let proj = tmp.path().join("my-robot");
    pin(&proj, "0.7.0-nros3");

    let (ok, text) = run(&launcher, &proj, &store, &["--version"]);
    assert!(!ok, "an unanswerable pin must not succeed:\n{text}");
    assert!(text.contains("0.7.0-nros3"), "{text}");
    assert!(text.contains("nros-toolchain.toml"), "{text}");
    assert!(
        text.contains("install.sh"),
        "the refusal must name the remedy:\n{text}"
    );
}
