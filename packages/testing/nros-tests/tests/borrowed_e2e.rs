//! Borrowed-view (RFC-0033, issue 0021 / #0423) RUNTIME E2E — build-stage form.
//!
//! The C and C++ proof binaries are LINKED at the fixture stage — the
//! `borrowed_e2e` `[[compile_check_fixture]]` row (builder `fixture-script`,
//! `fixtures/borrowed-e2e/build.sh`; issue 1656 gave it the row, so every
//! fixture lane builds it and its `.inputsig` can call it stale);
//! this test only RUNS them (no compilation at test time — the E1 rule). Each
//! driver owned-serializes a message, `deserialize_view`s it, and asserts every
//! borrowed view (C `nros/view.h` helpers; C++ `nros::Span`/`StringView`/`LeSpan`)
//! ALIASES the CDR buffer with correct values — printing `all views alias the CDR
//! buffer` on success and returning non-zero on any failed assertion.
//!
//! This replaces the orphaned+bit-rotted `tests/borrowed_{c,cpp}_e2e.sh` (#0423).
//! The two rots that killed those — the RFC-0042 platform.h move and the
//! `nros_config_variant_sz_*` guard (a standalone `nros-c` can't size the executor,
//! so its archive lacks the anchor the config header imports) — are handled in the
//! recipe: it adds the nros-platform-api include and links a matching WEAK variant
//! anchor (the guard is EXECUTOR-size-based, and borrowed views touch only the CDR
//! buffer via nros_serdes, so a borrowed-only consumer legitimately provides its
//! own weak anchor — exactly what nros-build-helpers emits it weak for).

use nros_tests::TestResult;
use std::{path::PathBuf, process::Command};

/// The fixture row's id in `examples/fixtures.toml`.
const ROW: &str = "borrowed_e2e";

fn proof_bin(name: &str) -> TestResult<PathBuf> {
    // Issue 1685 — a host (linux) build-stage proof: a lane that selects no
    // linux coordinate deselects here rather than reporting the absent stamp.
    nros_tests::fixtures::lane::require_platform_in_lane(
        &[nros_tests::matrix::PlatformId::Linux],
        "the borrowed-view proof",
    );
    // issue 1656 — the shared resolver: missing is a hard failure in a gated
    // run, `.build-failed` reports a broken build, a stale `.inputsig` is STALE.
    let stamp = nros_tests::fixtures::require_compile_check(ROW)?;
    let dir = stamp
        .parent()
        .expect("a stamp lives in its row dir")
        .to_path_buf();
    // A language whose host compiler was ABSENT at build time is a recorded
    // fact (`<bin>.skipped`, written by the build script), not a missing file.
    if let Ok(why) = std::fs::read_to_string(dir.join(format!("{name}.skipped"))) {
        nros_tests::skip!("borrowed-e2e proof `{name}` not built: {}", why.trim());
    }
    nros_tests::fixtures::require_compile_check_bin(ROW, name)
}

/// Run a prebuilt borrowed proof and assert it reports all views alias the buffer.
fn run_proof(bin: &std::path::Path, lang: &str) {
    let out = Command::new(bin)
        .output()
        .unwrap_or_else(|e| panic!("spawn {lang} borrowed proof {}: {e}", bin.display()));
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "{lang} borrowed proof exited non-zero — a borrowed view did NOT alias the CDR \
         buffer (or a value was wrong).\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("all views alias the CDR buffer"),
        "{lang} borrowed proof did not print the success marker.\nstdout:\n{stdout}"
    );
}

#[test]
fn c_borrowed_views_alias_the_cdr_buffer() -> TestResult<()> {
    let bin = proof_bin("borrowed_c_e2e")?;
    run_proof(&bin, "C");
    Ok(())
}

#[test]
fn cpp_borrowed_views_alias_the_cdr_buffer() -> TestResult<()> {
    let bin = proof_bin("borrowed_cpp_e2e")?;
    run_proof(&bin, "C++");
    Ok(())
}
