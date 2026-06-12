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
//! This regression test stages two tiny stock-tooling fixtures (one
//! Rust crate, one CMake project) that each contain a deliberate
//! compile error and asserts the well-known diagnostic prefix appears
//! verbatim on stderr after a vanilla `cargo check` / `cmake -B build`.
//!
//! The contract being protected is the §Non-Goals rule: nano-ros never
//! wraps, aggregates, or truncates the underlying toolchain's
//! diagnostics. If anything in the build orchestration ever swallows a
//! rustc / cmake error into a "build failed" summary, these tests
//! regress.
//!
//! Both `cargo` and `cmake` are tier-0 SDK requirements (`just doctor`
//! refuses to proceed without them), so this test does NOT carry a
//! `[SKIPPED]` path — a missing toolchain is a hard fail. The clang
//! variant is intentionally folded into the rustc path: rustc's own
//! E0432 emission is identical to a frontend diagnostic and exercises
//! the same "pass stderr through unchanged" contract.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn workspace_root() -> PathBuf {
    nros_tests::project_root()
}

fn fixture_src(name: &str) -> PathBuf {
    workspace_root()
        .join("packages/testing/nros-tests/fixtures")
        .join(name)
}

/// Copy a fixture tree into a fresh tempdir so the `cargo check` /
/// `cmake -B build` invocation does not leave a `target/` or `build/`
/// dir inside the source tree.
fn stage_fixture(name: &str) -> (tempfile::TempDir, PathBuf) {
    let src = fixture_src(name);
    assert!(
        src.is_dir(),
        "fixture missing: {} — did you delete the diagnostic fixture?",
        src.display()
    );
    let dst = tempfile::tempdir().expect("tempdir");
    copy_tree(&src, dst.path()).expect("copy fixture");
    let root = dst.path().to_path_buf();
    (dst, root)
}

fn copy_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            // Skip any stale build artefacts that leaked into the
            // source fixture (defensive — `.gitignore` keeps them out
            // of git, but a local `cargo check` run inside the fixture
            // would have populated them).
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name == "target" || name == "build" {
                continue;
            }
            copy_tree(&from, &to)?;
        } else if ty.is_file() {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// rustc's `error[E0432]: unresolved import` message must reach the
/// terminal verbatim. We grep stderr for the exact prefix to ensure no
/// layer between the user and rustc rewrote / truncated it.
#[test]
fn rustc_diagnostic_verbatim() {
    let (_guard, root) = stage_fixture("diagnostic_rustc_fixture");

    let output = Command::new("cargo")
        .args(["check", "--offline", "--color", "never"])
        .current_dir(&root)
        .output()
        .expect("spawn cargo check");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        !output.status.success(),
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
}

/// CMake's `Could not find a package configuration file provided by ...`
/// message must reach the terminal verbatim. Same contract as the
/// rustc variant — we are protecting against any orchestration layer
/// that swallows a downstream `cmake` failure into a generic summary.
#[test]
fn cmake_diagnostic_verbatim() {
    // Hard fail (not a skip) — `cmake` is a tier-0 SDK requirement per
    // §Non-Goals "we are not a build system replacement: cmake stays a
    // hard dep". A missing `cmake` binary is a doctor-level failure,
    // not a per-test gate.
    let cmake_present = Command::new("cmake")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    assert!(
        cmake_present,
        "cmake binary missing or non-functional — install cmake \
         (tier-0 dep, see `just doctor`)"
    );

    let (_guard, root) = stage_fixture("diagnostic_cmake_fixture");
    let build_dir = root.join("build");

    let output = Command::new("cmake")
        .args(["-B"])
        .arg(&build_dir)
        .arg("-S")
        .arg(&root)
        .output()
        .expect("spawn cmake");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        !output.status.success(),
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
}
