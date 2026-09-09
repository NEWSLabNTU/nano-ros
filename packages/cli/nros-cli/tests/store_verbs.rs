//! The store verbs as a USER reaches them — through the real `nros` binary
//! (phase-440 W6).
//!
//! `nros-cli-core/tests/store_reclaim.rs` covers what these verbs decide; this
//! file covers whether they can be invoked at all, which is a different
//! question and was briefly answered "no".
//!
//! `nros toolchain uninstall <ver>` panicked at startup in every build: the
//! binary sets `propagate_version = true`, clap therefore generates a
//! `--version` flag on every subcommand, and the positional field named
//! `version` collided with it — `Argument names must be unique`. Twelve tests
//! passed throughout, because each of them constructs the `Args` struct and
//! calls `run` directly, so nothing in the suite ever built the clap command.
//! A verb that cannot parse is not a verb.
//!
//! So these run the binary. They stay cheap by asking only for `--help` and for
//! a dry run against an EMPTY store — no fixtures, no provisioning, no `$HOME`.

use std::{path::PathBuf, process::Command};

fn nros() -> Command {
    Command::new(env!("CARGO_BIN_EXE_nros"))
}

fn run(args: &[&str]) -> (bool, String) {
    let out = nros().args(args).output().expect("failed to exec nros");
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.success(), text)
}

/// Every store verb builds its clap command. `--help` is the cheapest way to
/// demand that: clap's `debug_assert` over the whole command tree runs before
/// any of them can print anything.
#[test]
fn every_store_verb_parses() {
    for args in [
        vec!["store", "--help"],
        vec!["store", "list", "--help"],
        vec!["store", "gc", "--help"],
        vec!["toolchain", "--help"],
        vec!["toolchain", "uninstall", "--help"],
    ] {
        let (ok, text) = run(&args);
        assert!(ok, "`nros {}` failed:\n{text}", args.join(" "));
    }
}

/// The safe default is visible in the help, not only in the behaviour — a user
/// deciding whether to type `--delete` reads this text, and it is the only
/// warning they get.
#[test]
fn gc_help_says_the_default_removes_nothing() {
    let (ok, text) = run(&["store", "gc", "--help"]);
    assert!(ok, "{text}");
    assert!(
        text.contains("--delete"),
        "gc must document the opt-in it requires:\n{text}"
    );
    assert!(
        text.to_lowercase().contains("dry"),
        "gc must say it is a dry run by default:\n{text}"
    );
}

/// A duration with no unit is refused rather than read as seconds. Checked
/// through the binary because this is the message a user actually sees, and the
/// number in it is theirs.
#[test]
fn gc_refuses_a_duration_with_no_unit() {
    let (ok, text) = run(&["store", "gc", "--older-than", "90"]);
    assert!(!ok, "a unit-less duration must fail:\n{text}");
    assert!(
        text.contains("90d"),
        "the refusal must offer the spelling the user probably meant:\n{text}"
    );
}

/// `list` on a store that does not exist reports that and exits 0 — an
/// inspection verb must not fail on a host that has provisioned nothing.
///
/// `--root` keeps this off the real store: the verb reads `$NROS_STORE` /
/// `$NROS_HOME` / `$HOME` otherwise, and a test must not depend on, or disturb,
/// whatever this machine has installed.
#[test]
fn listing_an_absent_store_is_not_an_error() {
    // A path that does not exist and is never created — no tempdir crate, and
    // nothing to clean up, because the whole point is that the verb finds
    // nothing there and leaves it that way.
    let absent: PathBuf = std::env::temp_dir().join(format!(
        "nros-store-absent-{}-{}",
        std::process::id(),
        line!()
    ));
    assert!(!absent.exists(), "the probe path must start absent");
    let (ok, text) = run(&["store", "list", "--root", absent.to_str().unwrap()]);
    assert!(ok, "list on an absent store failed:\n{text}");
    assert!(
        text.contains("nothing is provisioned"),
        "it should say what it found:\n{text}"
    );
    assert!(
        !absent.exists(),
        "an inspection verb must not create the store it was asked about"
    );
}
