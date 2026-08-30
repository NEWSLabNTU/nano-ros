//! Borrowed-view (RFC-0033, issue 0021 / #0423) RUNTIME E2E — build-stage form.
//!
//! The C and C++ proof binaries are LINKED at the fixture stage
//! (`scripts/build/borrowed-e2e-fixture.sh` → `build/borrowed-e2e/borrowed_{c,cpp}_e2e`);
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

fn proof_bin(name: &str) -> TestResult<PathBuf> {
    // Bespoke recipe (like link-determinism), so the fixture lives at a fixed path
    // rather than under build/compile-check-fixtures/<id>/. Gate on its stamp.
    let dir = nros_tests::project_root().join("build/borrowed-e2e");
    if !dir.join(".compile-ok").is_file() {
        nros_tests::skip!(
            "borrowed-e2e fixture not built (build/borrowed-e2e/.compile-ok) — run \
             `scripts/build/borrowed-e2e-fixture.sh` (or `just check borrowed-e2e`) first"
        );
    }
    let bin = dir.join(name);
    if !bin.is_file() {
        nros_tests::skip!(
            "borrowed-e2e proof `{name}` not built — its host compiler was absent at \
             fixture-build time"
        );
    }
    Ok(bin)
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
