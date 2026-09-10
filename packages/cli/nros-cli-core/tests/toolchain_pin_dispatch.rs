//! The per-project pin and the launcher that dispatches by it — phase-440 W7,
//! RFC-0095 D7/D8/D9.
//!
//! Every test here builds a FAKE STORE in a tempdir and asks a pure function
//! about it. Nothing reads `$NROS_HOME`, `$NROS_STORE` or the process working
//! directory: the store root, the running-binary path and the project directory
//! are all parameters, so these run in parallel with everything else and observe
//! nothing global (issue 1101's hazard, and the rule `store_reclaim.rs` already
//! follows one module over).
//!
//! The three properties the wave was asked to prove, and where:
//!
//! * a pin is written on first build and NOT moved by a second —
//!   `first_build_writes_a_pin` + `a_second_build_does_not_move_the_pin`;
//! * a pin is found from a SUBDIRECTORY — `a_pin_is_found_from_a_subdirectory`
//!   and, through the launcher, `dispatch_finds_the_pin_from_a_subdirectory`;
//! * a launcher OLDER than the toolchain it launches still dispatches —
//!   `an_older_launcher_dispatches_to_a_newer_toolchain`.

use std::path::{Path, PathBuf};

use nros_cli_core::orchestration::{
    dispatch::{self, Context, Decision},
    pin::{self, PinOutcome},
};
use nros_launcher::session::Session;

/// A developer at a keyboard — the session every test here means unless it is
/// ABOUT RFC-0097 D11. Passed explicitly rather than detected, so a run on a
/// machine that exports `$CI` (this repo's own CI does) measures the same thing
/// as a run on a laptop.
fn dev() -> Session {
    Session::interactive()
}

/// A store entry that looks like one `scripts/install.sh` wrote: a prefix with
/// `bin/nros` in it. Returns the binary path, which is what a launcher execs
/// and what `pin::running_version` reads a version out of.
fn install_toolchain(store: &Path, version: &str) -> PathBuf {
    let bin = store.join("sdk").join("nros").join(version).join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let exe = bin.join("nros");
    std::fs::write(&exe, b"#!/bin/sh\nexit 0\n").unwrap();
    exe
}

/// A user's project: a directory with nothing nano-ros in it. Explicitly NOT a
/// checkout — no `packages/core/nros-core/Cargo.toml` anywhere above it, which
/// the tempdir root guarantees.
fn project(dir: &Path, rel: &str) -> PathBuf {
    let p = dir.join(rel);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn ctx(exe: &Path, cwd: &Path, store: &Path) -> Context {
    Context {
        exe: exe.to_path_buf(),
        cwd: cwd.to_path_buf(),
        store: store.to_path_buf(),
        dispatched: None,
        skip: false,
    }
}

// ---------------------------------------------------------------------------
// D9 — pin on first build
// ---------------------------------------------------------------------------

/// RFC-0095 D9's first half: a project with no pin gets one, naming the version
/// that actually built it — read off the running binary's store prefix, not off
/// the crate version, so the string names a directory.
#[test]
fn first_build_writes_a_pin() {
    let tmp = tempfile::tempdir().unwrap();
    let store = tmp.path().join("store");
    let exe = install_toolchain(&store, "0.5.0-nros1");
    let proj = project(tmp.path(), "my-robot");

    let outcome = pin::pin_on_first_build(&proj, &exe, &dev()).unwrap();
    let PinOutcome::Wrote { path, version } = outcome else {
        panic!("expected a pin to be written, got {outcome:?}");
    };
    assert_eq!(version, "0.5.0-nros1");
    assert_eq!(path, proj.join("nros-toolchain.toml"));
    assert_eq!(pin::load(&path).unwrap().version, "0.5.0-nros1");
}

/// The half that is easy to leave out, and the reason D9 cites `Cargo.lock`:
/// writing "the version it used" on EVERY build would move the pin the first
/// time the launcher was updated, which is the silent rebuild RFC-0095 D7
/// forbids. A second build with a DIFFERENT binary must leave the file alone.
///
/// This is also the `nros self update` invariant, tested at the only place that
/// writes a pin: an update moves the fronted binary, which is exactly what the
/// second `exe` below is.
#[test]
fn a_second_build_does_not_move_the_pin() {
    let tmp = tempfile::tempdir().unwrap();
    let store = tmp.path().join("store");
    let old = install_toolchain(&store, "0.5.0-nros1");
    let new = install_toolchain(&store, "0.6.0-nros1");
    let proj = project(tmp.path(), "my-robot");

    pin::pin_on_first_build(&proj, &old, &dev()).unwrap();
    let before = std::fs::read_to_string(proj.join("nros-toolchain.toml")).unwrap();

    let outcome = pin::pin_on_first_build(&proj, &new, &dev()).unwrap();
    let PinOutcome::Already(p) = &outcome else {
        panic!("a second build must find the pin, not rewrite it: {outcome:?}");
    };
    assert_eq!(p.version, "0.5.0-nros1");
    let after = std::fs::read_to_string(proj.join("nros-toolchain.toml")).unwrap();
    assert_eq!(before, after, "the pin file changed on a second build");
}

/// `write` itself refuses, so the only-if-absent rule does not depend on every
/// caller remembering to check first.
#[test]
fn write_refuses_to_overwrite_a_pin() {
    let tmp = tempfile::tempdir().unwrap();
    pin::write(tmp.path(), "0.5.0-nros1").unwrap();
    let err = pin::write(tmp.path(), "9.9.9-nros9")
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("refusing to overwrite"),
        "expected a refusal naming the rule, got: {err}"
    );
    assert_eq!(
        pin::load(&tmp.path().join("nros-toolchain.toml"))
            .unwrap()
            .version,
        "0.5.0-nros1"
    );
}

/// RFC-0095's own open question — *"it must survive `nros build` invoked from a
/// subdirectory"*. One pin at the root, found from three levels down, and NOT a
/// second pin written beside the subdirectory.
#[test]
fn a_pin_is_found_from_a_subdirectory() {
    let tmp = tempfile::tempdir().unwrap();
    let store = tmp.path().join("store");
    let exe = install_toolchain(&store, "0.5.0-nros1");
    let root = project(tmp.path(), "my-robot");
    let deep = project(tmp.path(), "my-robot/src/talker_pkg/src");

    pin::pin_on_first_build(&root, &exe, &dev()).unwrap();
    let found = pin::find(&deep).expect("the walk up must reach the root pin");
    assert_eq!(found, root.join("nros-toolchain.toml"));

    let outcome = pin::pin_on_first_build(&deep, &exe, &dev()).unwrap();
    assert!(
        matches!(outcome, PinOutcome::Already(_)),
        "a build from a subdirectory must use the root pin, not write its own: {outcome:?}"
    );
    assert!(
        !deep.join("nros-toolchain.toml").exists(),
        "a second pin was written at {}",
        deep.display()
    );
}

/// A binary with no store version — a contributor's `packages/cli/target/**`
/// build run outside its checkout — must not invent one. Pinning a version that
/// is in no store is worse than floating: the next build dispatches to a
/// directory nobody can create.
#[test]
fn a_binary_with_no_store_version_pins_nothing_and_says_so() {
    let tmp = tempfile::tempdir().unwrap();
    let exe = tmp.path().join("elsewhere/target/debug/nros");
    std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
    std::fs::write(&exe, b"x").unwrap();
    let proj = project(tmp.path(), "my-robot");

    let outcome = pin::pin_on_first_build(&proj, &exe, &dev()).unwrap();
    assert_eq!(outcome, PinOutcome::NoRunningVersion);
    assert!(!proj.join("nros-toolchain.toml").exists());
    assert!(
        pin::describe(&outcome).unwrap().contains("UNPINNED"),
        "the user must be told they still have D9's bug"
    );
}

// ---------------------------------------------------------------------------
// RFC-0097 D11 (phase-443 W4) — CI refuses to WRITE a pin
// ---------------------------------------------------------------------------

/// The acceptance clause, literally: an automated session does not mutate the
/// project's source, and the failure text names the fix.
///
/// `nros toolchain install` has no pin-writing path of its own — `nros build`
/// is the only thing in the tree that writes `nros-toolchain.toml` — so this is
/// where the whole rule lives.
#[test]
fn an_automated_session_refuses_to_write_a_pin_and_names_the_fix() {
    let tmp = tempfile::tempdir().unwrap();
    let store = tmp.path().join("store");
    let exe = install_toolchain(&store, "0.5.0-nros1");
    let proj = project(tmp.path(), "my-robot");

    let outcome = pin::pin_on_first_build(&proj, &exe, &Session::automated("CI")).unwrap();
    let PinOutcome::RefusedInCi { version, var, .. } = &outcome else {
        panic!("expected a refusal, got {outcome:?}");
    };
    assert_eq!(version, "0.5.0-nros1");
    assert_eq!(*var, "CI");
    assert!(
        outcome.is_refusal(),
        "the caller must be able to turn this into an ERROR — a warning here \
         goes green against a toolchain nobody recorded"
    );
    assert!(
        !proj.join("nros-toolchain.toml").exists(),
        "a refusal that leaves the file behind is not a refusal"
    );

    let text = pin::describe(&outcome).unwrap();
    assert!(
        text.contains("$CI"),
        "name the variable that decided:\n{text}"
    );
    assert!(
        text.contains("version = \"0.5.0-nros1\""),
        "print the pin to write, so the fix is a copy-paste:\n{text}"
    );
    assert!(
        text.contains("NROS_ALLOW_PIN_WRITE_IN_CI"),
        "name the escape hatch:\n{text}"
    );
}

/// The other half: *"a CI job that resolves a pin gets the same toolchain a
/// dev's machine does."* An ALREADY-pinned project is untouched by the rule —
/// that is the reproducible case, and the common one, and refusing it would be
/// the opposite of what D11 asks for.
#[test]
fn an_existing_pin_is_read_identically_in_ci() {
    let tmp = tempfile::tempdir().unwrap();
    let store = tmp.path().join("store");
    let exe = install_toolchain(&store, "0.5.0-nros1");
    let proj = project(tmp.path(), "my-robot");
    pin::pin_on_first_build(&proj, &exe, &dev()).unwrap();
    let before = std::fs::read_to_string(proj.join("nros-toolchain.toml")).unwrap();

    let outcome = pin::pin_on_first_build(&proj, &exe, &Session::automated("CI")).unwrap();
    let PinOutcome::Already(p) = &outcome else {
        panic!("a pinned project must resolve in CI, not refuse: {outcome:?}");
    };
    assert_eq!(p.version, "0.5.0-nros1");
    assert_eq!(
        before,
        std::fs::read_to_string(proj.join("nros-toolchain.toml")).unwrap(),
        "reading a pin in CI must not rewrite it"
    );
}

/// A contributor's checkout is untouched by the rule, and this is not academic:
/// this repo's OWN CI builds inside a checkout, so a refusal ordered before the
/// checkout arm would have failed every in-tree build the day it landed.
#[test]
fn a_checkout_in_ci_is_not_a_pin_refusal() {
    let tmp = tempfile::tempdir().unwrap();
    let store = tmp.path().join("store");
    let exe = install_toolchain(&store, "0.5.0-nros1");
    let root = project(tmp.path(), "nano-ros");
    std::fs::create_dir_all(root.join("packages/core/nros-core")).unwrap();
    std::fs::write(root.join("packages/core/nros-core/Cargo.toml"), "").unwrap();
    let ws = project(tmp.path(), "nano-ros/examples/workspaces/rust");

    let outcome = pin::pin_on_first_build(&ws, &exe, &Session::automated("CI")).unwrap();
    assert!(
        matches!(outcome, PinOutcome::InCheckout(_)),
        "a checkout must be reported as one, in CI too: {outcome:?}"
    );
    assert!(!outcome.is_refusal());
}

/// "Someone who really means it." With the escape hatch the pin IS written, so
/// the hatch is a live path rather than a documented intention.
#[test]
fn the_escape_hatch_lets_an_automated_session_write_a_pin() {
    let tmp = tempfile::tempdir().unwrap();
    let store = tmp.path().join("store");
    let exe = install_toolchain(&store, "0.5.0-nros1");
    let proj = project(tmp.path(), "my-robot");

    let session = Session::detect_with(|k| match k {
        "CI" => Some("true".into()),
        "NROS_ALLOW_PIN_WRITE_IN_CI" => Some("1".into()),
        _ => None,
    });
    let outcome = pin::pin_on_first_build(&proj, &exe, &session).unwrap();
    assert!(
        matches!(outcome, PinOutcome::Wrote { .. }),
        "the hatch must actually write: {outcome:?}"
    );
    assert_eq!(
        pin::load(&proj.join("nros-toolchain.toml"))
            .unwrap()
            .version,
        "0.5.0-nros1"
    );
}

// ---------------------------------------------------------------------------
// D8 — dispatch
// ---------------------------------------------------------------------------

/// The property the whole wave turns on (RFC-0095 D8): *"it must keep working
/// when it is OLDER than what it launches"*.
///
/// The launcher is 0.5.0-nros1 — the version `$NROS_STORE/bin/nros` was fronted
/// at before the newer one was installed. The project pins 0.6.0-nros1. The
/// decision must be `Exec` at the NEWER binary, and it must be reached without
/// parsing anything: `decide` takes no `argv`, which is what makes "older" a
/// property this test can assert rather than a hope.
#[test]
fn an_older_launcher_dispatches_to_a_newer_toolchain() {
    let tmp = tempfile::tempdir().unwrap();
    let store = tmp.path().join("store");
    let launcher = install_toolchain(&store, "0.5.0-nros1");
    let newer = install_toolchain(&store, "0.6.0-nros1");
    let proj = project(tmp.path(), "my-robot");
    pin::write(&proj, "0.6.0-nros1").unwrap();

    let d = dispatch::decide(&ctx(&launcher, &proj, &store)).unwrap();
    assert_eq!(
        d,
        Decision::Exec {
            version: "0.6.0-nros1".to_string(),
            bin: newer,
        }
    );
}

/// The steady state, and the one that must cost nothing: a launcher that IS the
/// pinned version carries on in this process rather than exec'ing itself.
#[test]
fn the_pinned_launcher_runs_in_place() {
    let tmp = tempfile::tempdir().unwrap();
    let store = tmp.path().join("store");
    let exe = install_toolchain(&store, "0.5.0-nros1");
    let proj = project(tmp.path(), "my-robot");
    pin::write(&proj, "0.5.0-nros1").unwrap();

    assert!(matches!(
        dispatch::decide(&ctx(&exe, &proj, &store)).unwrap(),
        Decision::Proceed(_)
    ));
}

/// Dispatch answers the same subdirectory question the pin does — a user runs
/// `nros build` from inside a package, not always from the workspace root.
#[test]
fn dispatch_finds_the_pin_from_a_subdirectory() {
    let tmp = tempfile::tempdir().unwrap();
    let store = tmp.path().join("store");
    let launcher = install_toolchain(&store, "0.5.0-nros1");
    let newer = install_toolchain(&store, "0.6.0-nros1");
    let root = project(tmp.path(), "my-robot");
    let deep = project(tmp.path(), "my-robot/src/talker_pkg");
    pin::write(&root, "0.6.0-nros1").unwrap();

    assert_eq!(
        dispatch::decide(&ctx(&launcher, &deep, &store)).unwrap(),
        Decision::Exec {
            version: "0.6.0-nros1".to_string(),
            bin: newer,
        }
    );
}

/// The exec loop a launcher must not have. A store entry whose directory name
/// disagrees with the binary inside it would otherwise dispatch to itself
/// forever; the marker the child inherits ends it in one step.
#[test]
fn a_dispatched_child_never_dispatches_again() {
    let tmp = tempfile::tempdir().unwrap();
    let store = tmp.path().join("store");
    let launcher = install_toolchain(&store, "0.5.0-nros1");
    install_toolchain(&store, "0.6.0-nros1");
    let proj = project(tmp.path(), "my-robot");
    pin::write(&proj, "0.6.0-nros1").unwrap();

    let mut c = ctx(&launcher, &proj, &store);
    c.dispatched = Some("0.6.0-nros1".to_string());
    assert!(matches!(
        dispatch::decide(&c).unwrap(),
        Decision::Proceed(_)
    ));
}

/// RFC-0095 D0/D5: a contributor inside a nano-ros checkout uses that clone's
/// own build, pin or no pin. A pin sitting in an in-tree workspace — a fixture,
/// a test, a user's project someone copied in — must not aim them at the store.
#[test]
fn a_checkout_is_never_dispatched_away_from() {
    let tmp = tempfile::tempdir().unwrap();
    let store = tmp.path().join("store");
    let launcher = install_toolchain(&store, "0.5.0-nros1");
    install_toolchain(&store, "0.6.0-nros1");
    // The marker `abi_guard::find_monorepo_root` looks for.
    let checkout = project(tmp.path(), "nano-ros");
    std::fs::create_dir_all(checkout.join("packages/core/nros-core")).unwrap();
    std::fs::write(
        checkout.join("packages/core/nros-core/Cargo.toml"),
        "[package]\nname = \"nros-core\"\n",
    )
    .unwrap();
    let ws = project(tmp.path(), "nano-ros/examples/workspaces/rust");
    pin::write(&ws, "0.6.0-nros1").unwrap();

    let d = dispatch::decide(&ctx(&launcher, &ws, &store)).unwrap();
    assert_eq!(
        d,
        Decision::Proceed("the cwd is inside a nano-ros checkout")
    );
    // And the same directory pins nothing, for the same reason.
    assert!(matches!(
        pin::pin_on_first_build(&ws, &launcher, &dev()).unwrap(),
        PinOutcome::InCheckout(_)
    ));
}

/// A pin naming a version the store does not have is `Missing`, carrying the
/// paths it looked in — never a silent proceed, which would build the project
/// with the wrong toolchain and never say so.
#[test]
fn a_pin_the_store_cannot_answer_is_reported_with_the_paths() {
    let tmp = tempfile::tempdir().unwrap();
    let store = tmp.path().join("store");
    let launcher = install_toolchain(&store, "0.5.0-nros1");
    let proj = project(tmp.path(), "my-robot");
    pin::write(&proj, "0.6.0-nros1").unwrap();

    let Decision::Missing {
        version, looked_in, ..
    } = dispatch::decide(&ctx(&launcher, &proj, &store)).unwrap()
    else {
        panic!("an absent toolchain must be reported, not proceeded past");
    };
    assert_eq!(version, "0.6.0-nros1");
    // Both layouts: RFC-0095 D2's `toolchains/<version>` and the
    // `sdk/nros/<version>` `scripts/install.sh` writes today.
    assert!(
        looked_in
            .iter()
            .any(|p| p.starts_with(store.join("toolchains")))
    );
    assert!(looked_in.iter().any(|p| p.starts_with(store.join("sdk"))));
}

/// D8 job 2's implementation is the installer the asset carries, not a second
/// downloader in Rust. Assert the path it is looked for at, in both directions
/// — a release that stops staging it must break this test rather than degrade
/// to a launcher that cannot fetch and does not say why.
#[test]
fn the_fetch_is_the_installer_the_asset_carries() {
    let tmp = tempfile::tempdir().unwrap();
    let store = tmp.path().join("store");
    let exe = install_toolchain(&store, "0.5.0-nros1");
    assert_eq!(dispatch::bundled_installer(&exe), None);

    let share = exe.parent().unwrap().parent().unwrap().join("share/nros");
    std::fs::create_dir_all(&share).unwrap();
    std::fs::write(share.join("install.sh"), b"#!/bin/sh\n").unwrap();
    assert_eq!(
        dispatch::bundled_installer(&exe),
        Some(share.join("install.sh"))
    );
}

/// A project with no pin floats, and the launcher must be transparent about it
/// rather than guessing a version. `nros build` is what ends that state (D9).
#[test]
fn an_unpinned_project_proceeds_in_the_running_binary() {
    let tmp = tempfile::tempdir().unwrap();
    let store = tmp.path().join("store");
    let exe = install_toolchain(&store, "0.5.0-nros1");
    let proj = project(tmp.path(), "my-robot");

    assert_eq!(
        dispatch::decide(&ctx(&exe, &proj, &store)).unwrap(),
        Decision::Proceed("this project names no toolchain")
    );
}

/// W6's `nros toolchain uninstall` reads any string value in this file as a
/// version, deliberately, so that W7 could not disarm the delete guard by
/// choosing a key it did not guess. The schema W7 chose stays inside that
/// envelope — asserted here rather than assumed, because the failure mode is a
/// toolchain somebody was using being deleted.
#[test]
fn w6s_delete_guard_still_sees_this_schema() {
    let tmp = tempfile::tempdir().unwrap();
    let path = pin::write(tmp.path(), "0.5.0-nros1").unwrap();
    let source = nros_cli_core::orchestration::store::load_pin_file(&path).unwrap();
    assert!(
        source
            .rules
            .contains(&nros_cli_core::orchestration::store::PinRule::AnyVersion(
                "0.5.0-nros1".to_string()
            )),
        "the reclaim verbs cannot see the pin: {:?}",
        source.rules
    );
}
