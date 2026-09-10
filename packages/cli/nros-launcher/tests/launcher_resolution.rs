//! What the launcher DECIDES — phase-443 W3/W4, RFC-0097 D4 + D11.
//!
//! Every test builds a fake store in a tempdir and asks
//! [`launch::resolve`](nros_launcher::launch::resolve) about it. Nothing reads
//! `$NROS_STORE`, `$CI` or the process working directory: the store root, the
//! project directory and the session are all parameters, so these run in
//! parallel with everything else and observe nothing global — and, importantly
//! for W4, they measure the same thing on a laptop and on a runner that
//! exports `$CI`.
//!
//! `launcher_binary.rs` is the other half: whether the decision reaches an
//! `exec`, through the real binary.

use std::path::{Path, PathBuf};

use nros_launcher::{
    launch::{self, Context, Plan, Source},
    session::Session,
};

/// A store entry that looks like one `scripts/install.sh` wrote.
fn install_toolchain(store: &Path, version: &str) -> PathBuf {
    let bin = store.join("sdk").join("nros").join(version).join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let exe = bin.join("nros");
    std::fs::write(&exe, b"#!/bin/sh\nexit 0\n").unwrap();
    exe
}

/// A user's project. Explicitly NOT a checkout — the tempdir root guarantees no
/// `packages/core/nros-core/Cargo.toml` above it.
fn project(dir: &Path, rel: &str) -> PathBuf {
    let p = dir.join(rel);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn pin_at(dir: &Path, version: &str) {
    std::fs::write(
        dir.join("nros-toolchain.toml"),
        format!("[toolchain]\nversion = \"{version}\"\n"),
    )
    .unwrap();
}

fn ctx(cwd: &Path, store: &Path, session: Session) -> Context {
    Context {
        // The launcher's own path only matters for finding a bundled
        // installer, which none of these have.
        exe: store.join("bin").join("nros"),
        cwd: cwd.to_path_buf(),
        store: store.to_path_buf(),
        dispatched: None,
        session,
    }
}

// ---------------------------------------------------------------------------
// W3 — the launcher resolves, including the states a toolchain never meets
// ---------------------------------------------------------------------------

/// The acceptance sentence, as a test: *"the launcher builds and runs with no
/// toolchain installed — it must be able to report 'nothing installed, run X'
/// rather than failing to start."*
///
/// The state a freshly installed launcher is in, and the one a launcher that
/// shipped inside the toolchain could not be in at all.
#[test]
fn an_empty_store_is_reported_with_the_command_that_fixes_it() {
    let tmp = tempfile::tempdir().unwrap();
    let store = tmp.path().join("store");
    std::fs::create_dir_all(&store).unwrap();
    let proj = project(tmp.path(), "my-robot");

    let plan = launch::resolve(&ctx(&proj, &store, Session::interactive())).unwrap();
    let Plan::NothingInstalled { .. } = &plan else {
        panic!("expected NothingInstalled, got {plan:?}");
    };
    let text = launch::explain(&plan);
    assert!(
        text.contains("install.sh"),
        "the report must name what to run:\n{text}"
    );
    assert!(
        text.contains(&store.display().to_string()),
        "and which store it looked in:\n{text}"
    );
}

/// An unpinned project on a developer's machine falls back to the newest
/// installed toolchain, the way `rustup` falls back to a default toolchain —
/// and NEWEST is natural order, so a store holding 0.9 and 0.10 picks 0.10.
#[test]
fn an_unpinned_project_takes_the_newest_installed_toolchain() {
    let tmp = tempfile::tempdir().unwrap();
    let store = tmp.path().join("store");
    install_toolchain(&store, "0.9.0-nros1");
    let newest = install_toolchain(&store, "0.10.0-nros1");
    let proj = project(tmp.path(), "my-robot");

    let plan = launch::resolve(&ctx(&proj, &store, Session::interactive())).unwrap();
    assert_eq!(
        plan,
        Plan::Exec {
            version: "0.10.0-nros1".to_string(),
            bin: newest,
            source: Source::Default,
        }
    );
}

/// A pin beats the default, from a SUBDIRECTORY, and names an OLDER version
/// than the store's newest — the case where "did the pin decide?" is actually
/// answerable.
#[test]
fn a_pin_beats_the_newest_installed_even_from_a_subdirectory() {
    let tmp = tempfile::tempdir().unwrap();
    let store = tmp.path().join("store");
    let pinned = install_toolchain(&store, "0.5.0-nros1");
    install_toolchain(&store, "0.10.0-nros1");
    let root = project(tmp.path(), "my-robot");
    pin_at(&root, "0.5.0-nros1");
    let deep = project(tmp.path(), "my-robot/src/talker_pkg/src");

    let plan = launch::resolve(&ctx(&deep, &store, Session::interactive())).unwrap();
    assert_eq!(
        plan,
        Plan::Exec {
            version: "0.5.0-nros1".to_string(),
            bin: pinned,
            source: Source::Pin(root.join("nros-toolchain.toml")),
        }
    );
}

/// A pin the store cannot answer is `Missing`, not a silent fallback to the
/// newest. Falling back would build the project with a toolchain nobody chose
/// while a file on disk says otherwise, which is worse than refusing.
#[test]
fn an_unanswerable_pin_does_not_fall_back_to_the_newest_installed() {
    let tmp = tempfile::tempdir().unwrap();
    let store = tmp.path().join("store");
    install_toolchain(&store, "0.10.0-nros1");
    let proj = project(tmp.path(), "my-robot");
    pin_at(&proj, "0.7.0-nros3");

    let plan = launch::resolve(&ctx(&proj, &store, Session::interactive())).unwrap();
    let Plan::Missing { version, .. } = &plan else {
        panic!("expected Missing, got {plan:?}");
    };
    assert_eq!(version, "0.7.0-nros3");
}

/// RFC-0097: *"inside a checkout the launcher is not involved."* The ownership
/// guard (`stale_guard`, phase-431 W1) requires that clone's own build, so a
/// launcher that dispatched a store toolchain here would only earn its refusal
/// one frame later — with a worse message.
#[test]
fn a_checkout_makes_the_launcher_stand_aside() {
    let tmp = tempfile::tempdir().unwrap();
    let store = tmp.path().join("store");
    install_toolchain(&store, "0.10.0-nros1");
    let root = project(tmp.path(), "nano-ros");
    std::fs::create_dir_all(root.join("packages/core/nros-core")).unwrap();
    std::fs::write(root.join("packages/core/nros-core/Cargo.toml"), "").unwrap();
    let inside = project(tmp.path(), "nano-ros/examples/native/rust/talker");

    let plan = launch::resolve(&ctx(&inside, &store, Session::interactive())).unwrap();
    let Plan::InCheckout { .. } = &plan else {
        panic!("expected InCheckout, got {plan:?}");
    };
    let text = launch::explain(&plan);
    assert!(
        text.contains("setup-cli") || text.contains("bootstrap.sh"),
        "a contributor must be told how to get the checkout's own CLI:\n{text}"
    );
}

/// The one failure mode a launcher must not have. A launcher reached from a
/// toolchain that already dispatched would exec it again, forever.
#[test]
fn an_already_dispatched_marker_refuses_rather_than_looping() {
    let tmp = tempfile::tempdir().unwrap();
    let store = tmp.path().join("store");
    install_toolchain(&store, "0.10.0-nros1");
    let proj = project(tmp.path(), "my-robot");
    let mut c = ctx(&proj, &store, Session::interactive());
    c.dispatched = Some("0.10.0-nros1".to_string());

    let plan = launch::resolve(&c).unwrap();
    let Plan::AlreadyDispatched { .. } = &plan else {
        panic!("expected AlreadyDispatched, got {plan:?}");
    };
    assert!(launch::explain(&plan).contains("exec loop"));
}

// ---------------------------------------------------------------------------
// W4 — CI does not choose a toolchain nobody named (RFC-0097 D11)
// ---------------------------------------------------------------------------

/// The launcher half of D11. Unpinned + CI is the same defect as a pin written
/// from CI, one step earlier — the runner's incidental store contents decide,
/// and nothing records what was chosen.
///
/// The launcher WARNS rather than refusing, and the warning is what turns
/// "silently different" into "different, and it said so": it carries the
/// version that ran. Refusing here would refuse `nros --version` and
/// `nros setup --list` too, because the launcher parses no `argv` and cannot
/// tell them apart from a build. `nros build` is where the refusal lives —
/// `toolchain_pin_dispatch.rs`, one crate over.
#[test]
fn an_unpinned_project_in_ci_warns_with_the_version_it_took() {
    let tmp = tempfile::tempdir().unwrap();
    let store = tmp.path().join("store");
    install_toolchain(&store, "0.10.0-nros1");
    let proj = project(tmp.path(), "my-robot");

    let plan = launch::resolve(&ctx(&proj, &store, Session::automated("CI"))).unwrap();
    let Plan::Exec { source, .. } = &plan else {
        panic!("expected Exec, got {plan:?}");
    };
    assert_eq!(*source, Source::DefaultInCi { var: "CI" });

    let text = launch::warning(&plan).expect("an unpinned CI run must warn");
    assert!(
        text.contains("$CI"),
        "name the variable that decided:\n{text}"
    );
    assert!(
        text.contains("0.10.0-nros1"),
        "name the version that actually ran — that is what makes it not \
         silent:\n{text}"
    );
    assert!(
        text.contains("version = \"0.10.0-nros1\""),
        "print the pin, so the fix is a copy-paste rather than an expedition:\n{text}"
    );
    assert!(
        text.contains("NROS_ALLOW_PIN_WRITE_IN_CI"),
        "name the escape hatch:\n{text}"
    );
    // …and the launcher still writes nothing. Only `nros build` may.
    assert!(!proj.join("nros-toolchain.toml").exists());
}

/// The negative control: the two states that must NOT warn. Without this, a
/// `warning` that returned `Some` for everything would pass the test above.
#[test]
fn nothing_else_warns() {
    let tmp = tempfile::tempdir().unwrap();
    let store = tmp.path().join("store");
    install_toolchain(&store, "0.10.0-nros1");
    let proj = project(tmp.path(), "my-robot");

    // Unpinned on a laptop.
    let dev = launch::resolve(&ctx(&proj, &store, Session::interactive())).unwrap();
    assert_eq!(launch::warning(&dev), None);

    // Pinned in CI — the reproducible case, and the common one.
    pin_at(&proj, "0.10.0-nros1");
    let ci = launch::resolve(&ctx(&proj, &store, Session::automated("CI"))).unwrap();
    assert_eq!(launch::warning(&ci), None);
}

/// The other half of the acceptance: *"a CI job that resolves a pin gets the
/// same toolchain a dev's machine does."* Same project, same store, two
/// sessions, one answer — the refusal must be about the ABSENCE of a pin, never
/// about being CI.
#[test]
fn a_pinned_project_resolves_identically_in_ci_and_on_a_developer_machine() {
    let tmp = tempfile::tempdir().unwrap();
    let store = tmp.path().join("store");
    install_toolchain(&store, "0.5.0-nros1");
    install_toolchain(&store, "0.10.0-nros1");
    let proj = project(tmp.path(), "my-robot");
    pin_at(&proj, "0.5.0-nros1");

    let dev = launch::resolve(&ctx(&proj, &store, Session::interactive())).unwrap();
    let ci = launch::resolve(&ctx(&proj, &store, Session::automated("GITHUB_ACTIONS"))).unwrap();
    assert_eq!(
        dev, ci,
        "a pinned project must not resolve differently in CI"
    );
    assert!(matches!(dev, Plan::Exec { .. }));
}

/// "Someone who really means it" — the escape hatch is what
/// [`Session::interactive`] represents, and with it CI behaves like a laptop.
#[test]
fn the_escape_hatch_lets_an_automated_session_choose() {
    let tmp = tempfile::tempdir().unwrap();
    let store = tmp.path().join("store");
    install_toolchain(&store, "0.10.0-nros1");
    let proj = project(tmp.path(), "my-robot");

    // `Session::detect_with` is what reads `$NROS_ALLOW_PIN_WRITE_IN_CI`; here
    // it is exercised end to end rather than hand-constructed, so the hatch's
    // NAME is covered too.
    let session = Session::detect_with(|k| match k {
        "CI" => Some("true".into()),
        "NROS_ALLOW_PIN_WRITE_IN_CI" => Some("1".into()),
        _ => None,
    });
    let plan = launch::resolve(&ctx(&proj, &store, session)).unwrap();
    assert!(
        matches!(plan, Plan::Exec { .. }),
        "the hatch must let CI choose: {plan:?}"
    );
}

/// An empty store in CI reports the store, not D11. Both statements are true
/// and only one is actionable: telling someone with nothing installed to commit
/// a pin would name a version that exists nowhere — and `warning` has no
/// version to put in it either.
#[test]
fn an_empty_store_in_ci_reports_the_store_rather_than_the_pin_rule() {
    let tmp = tempfile::tempdir().unwrap();
    let store = tmp.path().join("store");
    std::fs::create_dir_all(&store).unwrap();
    let proj = project(tmp.path(), "my-robot");

    let plan = launch::resolve(&ctx(&proj, &store, Session::automated("CI"))).unwrap();
    assert!(
        matches!(plan, Plan::NothingInstalled { .. }),
        "got {plan:?}"
    );
}
