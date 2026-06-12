//! Phase 134.6 E2E.1 — Symbol parity gate.
//!
//! Asserts that for every `_z_f_link_*_<transport>` wrapper defined
//! in `libnros_rmw_zenoh_staticlib.a`, the matching
//! `_z_*_<transport>` impl is also defined (no `U` rows where the
//! wrapper says `T`). Wraps `scripts/check-zenoh-archive-symbols.sh`
//! and surfaces any non-zero exit code as a test failure.
//!
//! Pre-Phase-134 the POSIX CMake path shipped wrappers compiled
//! under `Z_FEATURE_LINK_UDP_MULTICAST=1` (upstream's CMake default)
//! while deleting `src/system/unix/network.c` from the build copy,
//! leaving the multicast impls undefined. Phase 134's canonical-
//! header contract + the new multicast aliases in
//! `platform_aliases.c` close the gap. This test regression-guards
//! the contract for every future change to `build.rs`.
//!
//! Phase 150.E rev3 — the staticlib is now read from the
//! deterministic POSIX fixture --target-dir at
//! `target-zenoh-fixture-posix/release/libnros_rmw_zenoh_staticlib.a`
//! (built by `just build-zenoh-posix-fixture`, pulled in by
//! `just build-test-fixtures`). The shared workspace
//! `target/release/libnros_rmw_zenoh_staticlib.a` was unreliable:
//! whichever feature set built last (e.g. a cross-target
//! `just threadx_riscv64 build-fixtures` pass under Phase 146.2's
//! `LinkPolicy::threadx()`) overwrote the file, making the symbol
//! parity assertion measure the wrong contract. Override the
//! resolved path via `NROS_TESTS_ZENOH_ARCHIVE=<abs/path>` for
//! out-of-tree consumers (e.g. point at a CMake build's installed
//! archive).
//!
//! Test FAILS (not skips) if the archive is missing — run
//! `just build-zenoh-posix-fixture` (or `just build-test-fixtures`).

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

const SCRIPT: &str = "scripts/check-zenoh-archive-symbols.sh";

fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR for nros-tests is .../packages/testing/nros-tests.
    // Walk up three components to the workspace root.
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .ancestors()
        .nth(3)
        .expect("workspace root above CARGO_MANIFEST_DIR")
        .to_path_buf()
}

/// Phase 150.E rev3 — deterministic + overridable archive path.
fn resolve_archive_path(root: &Path) -> Result<PathBuf, String> {
    if let Some(explicit) = env::var_os("NROS_TESTS_ZENOH_ARCHIVE") {
        let p = PathBuf::from(explicit);
        if p.is_file() {
            return Ok(p);
        }
        return Err(format!(
            "NROS_TESTS_ZENOH_ARCHIVE points at {} but the file is missing",
            p.display(),
        ));
    }
    for profile in ["release", "debug"] {
        let candidate = root
            .join("target-zenoh-fixture-posix")
            .join(profile)
            .join("libnros_rmw_zenoh_staticlib.a");
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(format!(
        "libnros_rmw_zenoh_staticlib.a fixture not built. Run:\n  \
         just build-zenoh-posix-fixture\n\
         (or `just build-test-fixtures` which pulls it in).\n\
         Override path via NROS_TESTS_ZENOH_ARCHIVE=<abs/path/to/archive>.\n\
         Workspace root searched: {}",
        root.display(),
    ))
}

#[test]
fn zenoh_archive_wrapper_impl_parity() {
    let root = workspace_root();
    let script = root.join(SCRIPT);
    // Issue #34 — the zenoh-posix fixture archive is built by
    // `just build-zenoh-posix-fixture` / `build-test-fixtures`, not by the light
    // host-integration lane. Skip cleanly there (NROS_FIXTURES_OPTIONAL set);
    // the full `test-all` tier still fails loudly on a missing/regressed archive.
    let archive = match resolve_archive_path(&root) {
        Ok(p) => p,
        Err(e) if std::env::var_os("NROS_FIXTURES_OPTIONAL").is_some() => {
            nros_tests::skip!("zenoh-posix archive fixture not built (light tier): {e}");
        }
        Err(e) => panic!("{e}"),
    };
    // Touch metadata to suppress unused-import warning if path is
    // dropped in a future refactor.
    let _ = fs::metadata(&archive);

    assert!(
        script.exists(),
        "Phase 134 regression script missing at {}: did the script get deleted?",
        script.display()
    );

    let output = Command::new("bash")
        .arg(&script)
        .arg(&archive)
        .current_dir(&root)
        .output()
        .expect("failed to execute check-zenoh-archive-symbols.sh");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        panic!(
            "Phase 134 archive parity gate FAILED.\n\
             Wrappers without matching impls indicate the canonical-header \
             contract has drifted — every defined `_z_f_link_*_<transport>` \
             wrapper must have a defined `_z_*_<transport>` impl in the same \
             archive.\n\n\
             stdout:\n{}\n\
             stderr:\n{}",
            stdout, stderr
        );
    }

    // Sanity-check the success-path output — make sure the script
    // actually exercised the transports we care about. A future
    // refactor that turns the script into a no-op would silently
    // pass this test otherwise.
    assert!(
        stdout.contains("udp_multicast"),
        "regression script did not report on udp_multicast — \
         did the TRANSPORTS list shrink? stdout:\n{}",
        stdout
    );
    assert!(
        stdout.contains("zenoh archive symbol parity: clean"),
        "regression script output missing success banner.\nstdout:\n{}",
        stdout
    );
}
