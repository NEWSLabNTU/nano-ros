//! `nros::main!()` misuse diagnostics + rebuild-tracking.
//!
//! **The compiles happen in the BUILD stage** (issue 1620). Each case is a
//! `cargo-check-verdict` row in `examples/fixtures.toml`: the build stage stages
//! the `n9_workspace` template, applies the case's `main.rs` / `system.toml`
//! overlay (the template's `cases/<id>/` — FILES since issue 1656, so the row's
//! `.inputsig` covers them), runs `cargo check`, and records the exit status and
//! stderr. These tests assert the recorded verdict.
//!
//! This file used to be the documented exception to "No compilation inside
//! tests", on the grounds that a compile that must FAIL cannot be a prebuilt
//! fixture. It can: the artifact is the verdict, not a binary. Running the
//! checks here also made the file contend with `cmake_platform_matrix` in a
//! full-crate parallel run — both failed together and passed alone (issue
//! 1620) — and needed the crates.io index at test time.

use nros_tests::{TestResult, fixtures::require_compile_verdict};

fn assert_fails_with(id: &str, what: &str, needles: &[&str]) -> TestResult<()> {
    let v = require_compile_verdict(id)?.outcome;
    assert!(
        !v.success(),
        "expected `cargo check` to fail ({what}), but it exited 0.\nstdout:\n{}\nstderr:\n{}",
        v.stdout,
        v.stderr,
    );
    for n in needles {
        assert!(
            v.stderr.contains(n),
            "expected the diagnostic to contain `{n}` ({what}), stderr:\n{}",
            v.stderr,
        );
    }
    Ok(())
}

#[test]
fn custom_tasks_on_owned_spin_emits_error() -> TestResult<()> {
    // `custom_tasks = [...]` is RTIC-only; the `demo_entry` fixture deploys
    // native (OwnedSpin), so it must fail with the documented diagnostic.
    assert_fails_with(
        "main_macro_misuse_custom_tasks",
        "`custom_tasks` outside RTIC",
        &["custom_tasks", "only valid for the RTIC framework"],
    )
}

#[test]
fn custom_tasks_empty_on_owned_spin_still_errors() -> TestResult<()> {
    // Empty list `custom_tasks = []` is also a misuse outside RTIC.
    assert_fails_with(
        "main_macro_misuse_custom_tasks_empty",
        "`custom_tasks = []` on OwnedSpin",
        &["custom_tasks", "only valid for the RTIC framework"],
    )
}

#[test]
fn unknown_board_emits_compile_error() -> TestResult<()> {
    // The overlay replaces the entry's `system.toml` (beside the manifest since
    // phase-445 W5) with a copy naming `frobnicator`; the test below keeps that
    // copy equal to the template but for the board — so a pass here is about
    // the board.
    assert_fails_with(
        "main_macro_misuse_unknown_board",
        "unknown board `frobnicator`",
        &["unknown board"],
    )
}

/// The `unknown_board` overlay is the template's `system.toml` with ONE line
/// changed. Issue 1656 turned a `sed` rewrite into a file, which made it an input
/// the row's signature hashes — and made it able to drift from the template it
/// copies. Reads sources only.
#[test]
fn unknown_board_overlay_differs_from_the_template_only_in_its_board() {
    let ws = nros_tests::project_root().join("packages/testing/nros-tests/fixtures/n9_workspace");
    let read = |rel: &str| {
        std::fs::read_to_string(ws.join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"))
    };
    // From `[system]` on: each file's leading comment explains itself.
    let body = |text: String| {
        let at = text
            .find("\n[system]\n")
            .expect("a [system] table header line");
        text[at + 1..].to_string()
    };
    let template = body(read("src/demo_entry/system.toml"));
    let overlay = body(read(
        "cases/main_macro_misuse_unknown_board/src/demo_entry/system.toml.case",
    ));
    assert!(
        template.contains("\nboard = \"native\"\n"),
        "the template entry no longer states `board = \"native\"` — the misuse row's \
         premise moved; update its overlay"
    );
    assert_eq!(
        overlay,
        template.replace("\nboard = \"native\"\n", "\nboard = \"frobnicator\"\n"),
        "the unknown-board overlay drifted from src/demo_entry/system.toml: it must be \
         the template from `[system]` on with only the board replaced"
    );
}

/// `nros::main!` resolves the model from the bringup's INPUTS when no build
/// system produced one.
///
/// This is the contract issue 0414 left open: the macro used to only LOOK for
/// `config/system_model.yaml` and fail "SystemModel not found" whenever it ran
/// without a build step that resolved first — which a plain `cargo check` of an
/// entry crate always is (entry crates carry no `build.rs`, so they do not even
/// get an `$OUT_DIR` to fall back to).
///
/// The row is staged WITHOUT `nros sync` and with `NROS_MODEL_DIR` unset: no
/// committed model, no build system, so the macro must resolve `system.toml` +
/// the launch file itself.
#[test]
fn resolves_the_model_from_inputs_without_a_build_system() -> TestResult<()> {
    let v = require_compile_verdict("main_macro_resolves_from_inputs")?.outcome;
    assert!(
        v.success(),
        "the macro must resolve the model from system.toml + launch when nothing \
         produced one:\nstdout:\n{}\nstderr:\n{}",
        v.stdout,
        v.stderr,
    );
    Ok(())
}

/// Touching the model the BUILD produced must force a re-check.
///
/// The row checks once (the prelude), rewrites the model `nros sync` resolved
/// into the staged workspace, and checks again; the second check must re-check
/// `demo_entry`, because the macro tracks the model it read.
///
/// The touch target is still the model, because that is what the macro tracks.
/// Touching `system.toml` would be the better test of the user-facing contract,
/// and does NOT force a re-check today: `nros::main!` consumes the resolved
/// artifact and never sees the inputs behind it. Recorded on issue 0414 rather
/// than asserted here, because asserting it would be asserting a wish.
#[test]
fn rebuilds_on_model_touch() -> TestResult<()> {
    let v = require_compile_verdict("main_macro_rebuilds_on_model_touch")?;
    let first = v.prelude.expect(
        "the rebuild row records its FIRST check as verdict.prelude.* — without it \
         there is no before/after, and a single fresh check would pass vacuously",
    );
    assert!(
        first.success(),
        "initial cargo check failed:\nstdout:\n{}\nstderr:\n{}",
        first.stdout,
        first.stderr,
    );
    let second = v.outcome;
    assert!(
        second.success(),
        "second cargo check (post-touch) failed:\nstdout:\n{}\nstderr:\n{}",
        second.stdout,
        second.stderr,
    );
    assert!(
        second.stderr.contains("Checking demo_entry")
            || second.stderr.contains("Compiling demo_entry"),
        "expected demo_entry to be re-checked after the model touch, stderr:\n{}",
        second.stderr,
    );
    Ok(())
}
