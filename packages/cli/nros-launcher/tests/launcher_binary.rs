//! The launcher as an ARTIFACT — phase-443 W3, RFC-0097 D4.
//!
//! `launcher_resolution.rs` covers what it decides. This file covers the three
//! claims that are about the binary itself, and that no unit test can make:
//!
//! * it **runs with no toolchain installed** — the state a freshly installed
//!   launcher is in, and the one the phase-440 W7 module could not be in,
//!   because it WAS the toolchain;
//! * its **own version is separately reportable** — without this the split
//!   buys nothing observable, since a user reading one version string cannot
//!   tell which of the two artifacts answered;
//! * a **toolchain update does not replace it** — the property the whole split
//!   exists for, measured as bytes on disk rather than asserted in prose.
//!
//! Cheap by construction: no fixtures, no provisioning, no network. The store
//! is a tempdir named by `$NROS_STORE`, and the toolchain is a shell script,
//! which is the right stand-in rather than a shortcut — what is under test is
//! the handover, and a script can PROVE what it received in a way a second copy
//! of a binary cannot.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use nros_launcher::test_support::{copy_executable, write_executable_stub};

/// The launcher, installed where `scripts/install.sh` fronts it: `<store>/bin/
/// nros`. A COPY, not a symlink into a toolchain prefix — that is the property
/// `a_toolchain_update_does_not_replace_the_launcher` measures.
///
/// Through `test_support`, never `std::fs::copy`: this test execs what it just
/// wrote, which is issue 0476's exact shape.
fn install_launcher(store: &Path) -> PathBuf {
    let bin = store.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let exe = bin.join("nros");
    copy_executable(Path::new(env!("CARGO_BIN_EXE_nros-launcher")), &exe);
    exe
}

/// A stand-in toolchain that reports what it was handed.
fn install_stub(store: &Path, version: &str) -> PathBuf {
    let bin = store.join("sdk/nros").join(version).join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let exe = bin.join("nros");
    write_executable_stub(
        &exe,
        &format!(
            "#!/bin/sh\n\
             echo \"nros {version} (STUB-TOOLCHAIN)\"\n\
             echo \"DISPATCH=${{NROS_TOOLCHAIN_DISPATCH:-unset}}\"\n\
             for a in \"$@\"; do echo \"ARG=$a\"; done\n"
        ),
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

/// Run the launcher with an environment nothing on the host can decide.
///
/// `CI` / `GITHUB_ACTIONS` are REMOVED unless a test asks for them: this suite
/// runs in CI, and a launcher that refuses an unpinned project there (RFC-0097
/// D11 — correctly) would otherwise make half of these tests measure the runner
/// rather than the code.
fn run_in(launcher: &Path, cwd: &Path, store: &Path, args: &[&str], ci: Option<&str>) -> Out {
    let mut cmd = Command::new(launcher);
    cmd.args(args)
        .current_dir(cwd)
        .env("NROS_STORE", store)
        .env_remove("NROS_HOME")
        .env_remove("CI")
        .env_remove("GITHUB_ACTIONS")
        .env_remove("NROS_ALLOW_PIN_WRITE_IN_CI")
        .env_remove("NROS_TOOLCHAIN_DISPATCH");
    if let Some(var) = ci {
        cmd.env(var, "true");
    }
    let out = cmd.output().expect("failed to run the launcher");
    Out {
        ok: out.status.success(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        all: format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    }
}

struct Out {
    ok: bool,
    stdout: String,
    all: String,
}

fn run(launcher: &Path, cwd: &Path, store: &Path, args: &[&str]) -> Out {
    run_in(launcher, cwd, store, args, None)
}

/// W3's first acceptance clause, literally: a launcher with NO toolchain
/// installed must start and say what to run.
///
/// It fails (exit non-zero) because it could not run the command — a launcher
/// that exited 0 here would read to CI as a build that passed — but it fails
/// with an answer, not with a crash.
#[test]
fn the_launcher_runs_with_no_toolchain_installed_and_says_what_to_run() {
    let tmp = tempfile::tempdir().unwrap();
    let store = tmp.path().join("store");
    let launcher = install_launcher(&store);
    let proj = tmp.path().join("my-robot");
    std::fs::create_dir_all(&proj).unwrap();

    let out = run(&launcher, &proj, &store, &["build"]);
    assert!(
        !out.ok,
        "a launcher that cannot run the command must not exit 0:\n{}",
        out.all
    );
    assert!(
        out.all.contains("no nano-ros toolchain is installed"),
        "expected the nothing-installed report:\n{}",
        out.all
    );
    assert!(
        out.all.contains("install.sh"),
        "the report must name what to run:\n{}",
        out.all
    );
}

/// W3's third acceptance clause: *"`nros --version` must distinguish launcher
/// from toolchain, or the split buys nothing observable."*
///
/// Two lines, in order: the launcher's own version, then whatever the toolchain
/// prints. The argument is still forwarded verbatim, which the `ARG=` line
/// proves — the launcher adds a line, it does not consume a flag.
#[test]
fn version_distinguishes_the_launcher_from_the_toolchain_it_launches() {
    let tmp = tempfile::tempdir().unwrap();
    let store = tmp.path().join("store");
    let launcher = install_launcher(&store);
    install_stub(&store, "0.6.0-nros1");
    let proj = tmp.path().join("my-robot");
    pin(&proj, "0.6.0-nros1");

    let out = run(&launcher, &proj, &store, &["--version"]);
    assert!(out.ok, "{}", out.all);
    assert!(
        out.stdout.contains("nros-launcher "),
        "the launcher must report its OWN version:\n{}",
        out.stdout
    );
    assert!(
        out.stdout.contains("nros 0.6.0-nros1 (STUB-TOOLCHAIN)"),
        "the toolchain's version must follow it:\n{}",
        out.stdout
    );
    assert!(
        out.stdout.find("nros-launcher ") < out.stdout.find("nros 0.6.0-nros1"),
        "launcher first, then the toolchain it launched:\n{}",
        out.stdout
    );
    assert!(
        out.all.contains("ARG=--version"),
        "`--version` must still be FORWARDED, not consumed:\n{}",
        out.all
    );
}

/// The launcher answers `--version` even with an empty store — and exits 0,
/// because the question it was asked was answered. This is the one place the
/// two acceptance clauses meet, and the reason the version line is printed
/// BEFORE the store is resolved.
#[test]
fn version_works_with_no_toolchain_installed() {
    let tmp = tempfile::tempdir().unwrap();
    let store = tmp.path().join("store");
    let launcher = install_launcher(&store);
    let proj = tmp.path().join("my-robot");
    std::fs::create_dir_all(&proj).unwrap();

    let out = run(&launcher, &proj, &store, &["--version"]);
    assert!(
        out.ok,
        "`nros --version` must not be a broken command on a host with an empty store:\n{}",
        out.all
    );
    assert!(out.stdout.contains("nros-launcher "), "{}", out.stdout);
    assert!(
        out.all.contains("no nano-ros toolchain is installed"),
        "and it must say why no toolchain line followed:\n{}",
        out.all
    );
}

/// The property the split exists for, measured rather than asserted: installing
/// a NEWER toolchain changes the store and leaves the launcher binary byte for
/// byte identical — while the launcher immediately starts handing over to it.
///
/// This is what "a toolchain update does not replace the launcher" means in a
/// world where both binaries are called `nros`.
#[test]
fn a_toolchain_update_does_not_replace_the_launcher() {
    let tmp = tempfile::tempdir().unwrap();
    let store = tmp.path().join("store");
    let launcher = install_launcher(&store);
    install_stub(&store, "0.6.0-nros1");
    let proj = tmp.path().join("my-robot");
    std::fs::create_dir_all(&proj).unwrap();

    let before = std::fs::read(&launcher).unwrap();
    let first = run(&launcher, &proj, &store, &["build"]);
    assert!(
        first.all.contains("nros 0.6.0-nros1 (STUB-TOOLCHAIN)"),
        "{}",
        first.all
    );

    // "Update the toolchain" — a second version lands in the store. Nothing
    // else happens; in particular nothing writes to `<store>/bin`.
    install_stub(&store, "0.7.0-nros1");

    let after = std::fs::read(&launcher).unwrap();
    assert_eq!(
        before, after,
        "installing a toolchain rewrote the launcher binary"
    );
    let second = run(&launcher, &proj, &store, &["build"]);
    assert!(
        second.all.contains("nros 0.7.0-nros1 (STUB-TOOLCHAIN)"),
        "the same launcher must now hand over to the newer toolchain:\n{}",
        second.all
    );
}

/// RFC-0097 D11 through the real binary: an unpinned project in an automated
/// session is told, on stderr, which toolchain ran and how to pin it — and the
/// toolchain still runs, because the launcher cannot tell `build` from
/// `--version` and refusing both is not enforcing D11.
///
/// The refusal belongs to `nros build`, which knows it is about to write source
/// (`toolchain_pin_dispatch.rs`, one crate over).
#[test]
fn an_unpinned_project_in_ci_is_warned_about_through_the_real_binary() {
    let tmp = tempfile::tempdir().unwrap();
    let store = tmp.path().join("store");
    let launcher = install_launcher(&store);
    install_stub(&store, "0.6.0-nros1");
    let proj = tmp.path().join("my-robot");
    std::fs::create_dir_all(&proj).unwrap();

    let out = run_in(&launcher, &proj, &store, &["build"], Some("GITHUB_ACTIONS"));
    assert!(
        out.all.contains("$GITHUB_ACTIONS"),
        "name the variable that decided:\n{}",
        out.all
    );
    assert!(
        out.all.contains("nros-toolchain.toml"),
        "name the fix:\n{}",
        out.all
    );
    assert!(
        out.all.contains("nros 0.6.0-nros1 (STUB-TOOLCHAIN)"),
        "the toolchain still runs — only `nros build` refuses:\n{}",
        out.all
    );
    assert!(
        !proj.join("nros-toolchain.toml").exists(),
        "the launcher must never write a pin file"
    );

    // The negative control, on the same store and the same runner: WITH a pin
    // there is nothing to warn about. Without this, a launcher that printed the
    // warning unconditionally would pass the assertions above.
    pin(&proj, "0.6.0-nros1");
    let pinned = run_in(&launcher, &proj, &store, &["build"], Some("GITHUB_ACTIONS"));
    assert!(
        pinned.all.contains("nros 0.6.0-nros1 (STUB-TOOLCHAIN)"),
        "a pinned project must build in CI exactly as on a laptop:\n{}",
        pinned.all
    );
    assert!(
        !pinned.all.contains("$GITHUB_ACTIONS"),
        "a pinned CI build must get no D11 warning at all:\n{}",
        pinned.all
    );
}
