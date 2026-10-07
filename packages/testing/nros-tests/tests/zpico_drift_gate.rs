//! Phase 136 E2E.3 / phase-290 — zenoh-pico source-list drift gate.
//!
//! `zpico-sys/build.rs` panics at build time if any `[build.zenoh]` source root
//! in a platform package's `nros-platform.toml` no longer resolves to a real
//! path under `zenoh-pico/src/`. The check is the structural firewall against
//! silent stale-source bugs when upstream zenoh-pico bumps rename
//! `system/<plat>/` dirs.
//!
//! This test guards the gate itself, and since issue 1656 it compiles nothing:
//! the two `cargo build -p zpico-sys` runs it used to make are two
//! `cargo-check-verdict` rows over `fixtures/zpico_drift_gate/`, which shadows
//! the posix descriptor through `NROS_PLATFORMS_DIR` (the first platform search
//! root; a descriptor is keyed by its directory name):
//!
//! * `zpico_drift_sentinel_include` — the canonical descriptor with every
//!   `system/unix` replaced by a sentinel path that cannot exist. The build
//!   script MUST fail, naming the sentinel. That also proves the override hook
//!   is honoured: if a refactor dropped the env read, the corrupted copy would
//!   be ignored and the row would compile.
//! * `zpico_drift_canonical_include` — the canonical descriptor verbatim. MUST
//!   compile, so the failure above is the corruption and not the override.
//!
//! The two copies are FILES in the fixture (a row's overlay is an input its
//! signature hashes), so they could drift from the descriptor they claim to
//! copy. The first test refuses that, reading sources only.

use std::{fs, path::PathBuf};

const SENTINEL: &str = "system/_zpico_drift_gate_sentinel_does_not_exist";

fn canonical_posix_descriptor() -> PathBuf {
    nros_tests::project_root().join("packages/platform/nros-platform-posix/nros-platform.toml")
}

fn fixture_copy(row: &str) -> PathBuf {
    nros_tests::project_root()
        .join("packages/testing/nros-tests/fixtures/zpico_drift_gate/cases")
        .join(row)
        .join("platforms/nros-platform-posix/nros-platform.toml.case")
}

/// The fixture's two descriptors are the canonical one — verbatim, and with
/// exactly the sentinel substitution. A canonical edit that is not mirrored
/// here would leave the build-stage rows measuring a descriptor nobody ships.
#[test]
fn fixture_descriptors_track_the_canonical_posix_descriptor() {
    let canonical = fs::read_to_string(canonical_posix_descriptor())
        .expect("read the canonical posix nros-platform.toml");
    assert!(
        canonical.contains("system/unix"),
        "the canonical posix descriptor no longer names `system/unix` — the \
         corruption this gate applies no longer reaches a source path; pick a \
         new one in fixtures/zpico_drift_gate and here"
    );
    let read = |row: &str| {
        fs::read_to_string(fixture_copy(row))
            .unwrap_or_else(|e| panic!("read {}: {e}", fixture_copy(row).display()))
    };
    let regenerate = "Regenerate from packages/platform/nros-platform-posix/nros-platform.toml: \
                      the canonical row is a verbatim copy, the sentinel row replaces every \
                      `system/unix` with the sentinel path.";
    assert_eq!(
        read("zpico_drift_canonical_include"),
        canonical,
        "fixtures/zpico_drift_gate: the canonical-include copy drifted. {regenerate}"
    );
    assert_eq!(
        read("zpico_drift_sentinel_include"),
        canonical.replace("system/unix", SENTINEL),
        "fixtures/zpico_drift_gate: the sentinel-include copy drifted. {regenerate}"
    );
}

#[test]
fn zpico_drift_gate_fires_on_corrupted_include() -> nros_tests::TestResult<()> {
    let corrupted = nros_tests::fixtures::require_compile_verdict("zpico_drift_sentinel_include")?;
    let out = &corrupted.outcome.stderr;
    assert!(
        !corrupted.outcome.success(),
        "expected the zpico-sys build script to fail with the corrupted platform \
         descriptor, but the check succeeded — either the drift gate or the \
         NROS_PLATFORMS_DIR override stopped working. Output:\n{out}"
    );
    assert!(
        out.contains("_zpico_drift_gate_sentinel_does_not_exist"),
        "the build failed, but its diagnostic does not name the corrupted path — \
         it failed for some other reason. Output:\n{out}"
    );

    // Round-trip: the canonical descriptor, through the same override, builds.
    let pristine = nros_tests::fixtures::require_compile_verdict("zpico_drift_canonical_include")?;
    assert!(
        pristine.outcome.success(),
        "the canonical posix descriptor, reached through NROS_PLATFORMS_DIR, should \
         build cleanly, but it failed. Output:\n{}",
        pristine.outcome.stderr
    );
    Ok(())
}
