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
//! Phase 140 — the test now points at the Corrosion-emitted staticlib
//! under `target/release/` instead of the legacy `build/install/`
//! prefix. Test FAILS (not skips) if the archive is missing — run
//! `cargo build -p nros-rmw-zenoh-staticlib --release` first.

use std::{path::Path, process::Command};

const ARCHIVE: &str = "target/release/libnros_rmw_zenoh_staticlib.a";
const SCRIPT: &str = "scripts/check-zenoh-archive-symbols.sh";

fn workspace_root() -> std::path::PathBuf {
    // CARGO_MANIFEST_DIR for nros-tests is .../packages/testing/nros-tests.
    // Walk up three components to the workspace root.
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .ancestors()
        .nth(3)
        .expect("workspace root above CARGO_MANIFEST_DIR")
        .to_path_buf()
}

#[test]
fn zenoh_archive_wrapper_impl_parity() {
    let root = workspace_root();
    let archive = root.join(ARCHIVE);
    let script = root.join(SCRIPT);

    assert!(
        script.exists(),
        "Phase 134 regression script missing at {}: did the script get deleted?",
        script.display()
    );
    assert!(
        archive.exists(),
        "libnros_rmw_zenoh_staticlib.a not found at {} — run \
         `cargo build -p nros-rmw-zenoh-staticlib --release` first \
         (Phase 134 contract presumes the staticlib is built)",
        archive.display()
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
