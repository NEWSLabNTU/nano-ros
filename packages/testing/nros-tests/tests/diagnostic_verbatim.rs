//! Phase 212 §Acceptance — toolchain diagnostic verbatim contract.
//!
//! Per `docs/roadmap/phase-212-ux-cargo-native-and-file-consolidation.md`
//! §Acceptance:
//!
//! > A failing rustc / cmake / clang diagnostic in any test fixture
//! > reaches the user's terminal verbatim — no aggregation, no
//! > truncation. CI test injects a synthetic compile error and greps
//! > for the original message.
//!
//! Two tiny stock-tooling fixtures (one Rust crate, one CMake project)
//! each contain a deliberate compile error; this regression test asserts
//! the well-known diagnostic prefix appears verbatim on the stderr of a
//! vanilla `cargo check` / `cmake -B build`.
//!
//! The contract being protected is the §Non-Goals rule: nano-ros never
//! wraps, aggregates, or truncates the underlying toolchain's
//! diagnostics. If anything in the build orchestration ever swallows a
//! rustc / cmake error into a "build failed" summary, these tests
//! regress.
//!
//! **Both compiles run in the BUILD stage** (issue 1620): the
//! `diagnostic_rustc_verbatim` (`cargo-check-verdict`) and
//! `diagnostic_cmake_verbatim` (`cmake-configure-verdict`) rows stage each
//! fixture, run a vanilla `cargo check` / `cmake -S . -B build`, and record the
//! exit status and stderr byte for byte. These tests assert the recorded
//! verdict. That keeps the contract under test — the toolchain's own text,
//! unwrapped — while removing the compile from test time.
//!
//! A missing toolchain fails the fixture BUILD (the builders refuse to record
//! a verdict without `cmake`), and a missing verdict fails the test hard in a
//! gated run (issue 0584) — there is no `[SKIPPED]` path, as before: both are
//! tier-0 SDK requirements (`just doctor`).

use nros_tests::{TestResult, fixtures::require_compile_verdict};

/// rustc's `error[E0432]: unresolved import` message must reach the
/// terminal verbatim. We grep stderr for the exact prefix to ensure no
/// layer between the user and rustc rewrote / truncated it.
#[test]
fn rustc_diagnostic_verbatim() -> TestResult<()> {
    let output = require_compile_verdict("diagnostic_rustc_verbatim")?.outcome;
    let (stdout, stderr) = (&output.stdout, &output.stderr);

    assert!(
        !output.success(),
        "cargo check unexpectedly succeeded — fixture lost its compile error.\n\
         stdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // The verbatim line we expect rustc to emit, character-for-character.
    let needle = "error[E0432]: unresolved import";
    assert!(
        stderr.contains(needle),
        "Phase 212 §Acceptance: rustc diagnostic verbatim contract violated.\n\
         Expected stderr to contain `{needle}` verbatim (no wrapping, no \
         truncation, no aggregation).\n\
         stdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Also confirm the identifier the user wrote is echoed back — guards
    // against a hypothetical wrapper that kept the error code but stripped
    // the offending span.
    let span_needle = "nonexistent_crate_for_phase212_verbatim_test";
    assert!(
        stderr.contains(span_needle),
        "Phase 212 §Acceptance: rustc diagnostic span elided.\n\
         Expected stderr to mention `{span_needle}`.\n\
         stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    Ok(())
}

/// CMake's `Could not find a package configuration file provided by ...`
/// message must reach the terminal verbatim. Same contract as the
/// rustc variant — we are protecting against any orchestration layer
/// that swallows a downstream `cmake` failure into a generic summary.
#[test]
fn cmake_diagnostic_verbatim() -> TestResult<()> {
    let output = require_compile_verdict("diagnostic_cmake_verbatim")?.outcome;
    let (stdout, stderr) = (&output.stdout, &output.stderr);

    assert!(
        !output.success(),
        "cmake unexpectedly succeeded — fixture lost its find_package error.\n\
         stdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // The verbatim line we expect cmake to emit. CMake renders the
    // failure as a multi-line block starting with this prefix.
    let needle = "Could not find a package configuration file provided by";
    assert!(
        stderr.contains(needle),
        "Phase 212 §Acceptance: cmake diagnostic verbatim contract violated.\n\
         Expected stderr to contain `{needle}` verbatim (no wrapping, no \
         truncation, no aggregation).\n\
         stdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Also confirm the package name the user wrote is echoed back —
    // guards against a wrapper that kept the boilerplate but stripped
    // the actionable identifier.
    let span_needle = "NoSuchPackageForPhase212VerbatimTest";
    assert!(
        stderr.contains(span_needle),
        "Phase 212 §Acceptance: cmake diagnostic identifier elided.\n\
         Expected stderr to mention `{span_needle}`.\n\
         stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    Ok(())
}
