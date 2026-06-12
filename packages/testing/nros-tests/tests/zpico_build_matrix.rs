//! Phase 136 E2E.1 + E2E.7 — `zpico-sys` build-matrix gate.
//!
//! E2E.1 — assert the prebuilt POSIX fixture archive contains
//! the `_z_f_link_*` symbol set the platform's `link.*` policy
//! resolves to. Pre-136 the per-RTOS branches in `build.rs` each
//! had their own symbol-set; post-136 the single
//! `build_zenoh_pico_unified` produces them from manifest data.
//! Symbol-set drift = regression.
//!
//! E2E.7 — `cargo tree -p zpico-sys | grep cmake` returns no rows.
//! Phase 136.3 deleted the `cmake = "0.1"` dep; this guards against
//! it sneaking back in via a transitive dep or revert.
//!
//! The matrix is intentionally narrow on the test runner. The
//! host-runnable POSIX archive is built by `just build-test-fixtures`
//! at `target-zenoh-fixture-posix/`; cross-compile builds for FreeRTOS
//! / NuttX / ThreadX / Zephyr embedded targets are covered by their
//! per-platform `just <plat> build-fixtures` recipes. Re-running any of
//! those builds inside nextest would double CI wall-clock for no extra
//! coverage.

use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

fn workspace_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .ancestors()
        .nth(3)
        .expect("workspace root above CARGO_MANIFEST_DIR")
        .to_path_buf()
}

/// E2E.7 — assert `cmake = "0.1"` is gone from `zpico-sys`'s dep
/// graph. `cargo tree --invert cmake -p zpico-sys` is the cleanest
/// shape; a non-empty result means cmake came back. We run the
/// inverted lookup so the test's stdout names the offending crate
/// directly on failure (instead of just "cmake appears in tree").
#[test]
fn zpico_sys_has_no_cmake_dep() {
    let root = workspace_root();
    let output = Command::new("cargo")
        .args(["tree", "-p", "zpico-sys", "--prefix=none", "--no-dedupe"])
        .current_dir(&root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("cargo tree failed to spawn");

    if !output.status.success() {
        panic!(
            "cargo tree -p zpico-sys exited non-zero. Stderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let cmake_lines: Vec<&str> = stdout
        .lines()
        .filter(|line| {
            // Match the `cmake` crate exactly, not e.g. `cmake-rs`
            // or `cmake_macros`. cargo tree formats deps as
            // `<name> v<ver>` so we anchor on a trailing space + 'v'.
            let trimmed = line.trim_start();
            trimmed.starts_with("cmake v") || trimmed == "cmake"
        })
        .collect();

    assert!(
        cmake_lines.is_empty(),
        "Phase 136.3 deleted the `cmake = \"0.1\"` build-dep from \
         zpico-sys. It came back via:\n{}",
        cmake_lines.join("\n")
    );
}

fn prebuilt_posix_archive(root: &Path) -> PathBuf {
    for profile in ["release", "debug"] {
        let archive = root
            .join("target-zenoh-fixture-posix")
            .join(profile)
            .join("libnros_rmw_zenoh_staticlib.a");
        if archive.is_file() {
            return archive;
        }
    }
    // Issue #34 — the zenoh-posix fixture archive is built by
    // `just build-zenoh-posix-fixture` / `build-test-fixtures`, not by the light
    // host-integration lane. Skip cleanly there (NROS_FIXTURES_OPTIONAL set);
    // the full `test-all` tier still fails loudly on the missing archive.
    if std::env::var_os("NROS_FIXTURES_OPTIONAL").is_some() {
        nros_tests::skip!(
            "zenoh-posix staticlib fixture not built (light tier); searched {}",
            root.join("target-zenoh-fixture-posix").display()
        );
    }
    panic!(
        "POSIX zenoh staticlib fixture not built. Run `just build-test-fixtures` \
         or `just build-zenoh-posix-fixture` first; searched {}",
        root.join("target-zenoh-fixture-posix").display()
    );
}

/// E2E.1 — POSIX platform archive built through the unified cc-rs path
/// emits the canonical link-feature symbols. The archive is prebuilt by
/// the fixture stage so `just test-all` does not compile it inside the
/// test body.
#[test]
fn zpico_posix_archive_carries_link_feature_symbols() {
    let root = workspace_root();
    let archive = prebuilt_posix_archive(&root);

    // POSIX's `LinkPolicy` enables tcp / udp_unicast / udp_multicast,
    // disables serial / ws / bluetooth / raweth / tls (link-tls is
    // opt-in). `_z_f_link_tcp` and friends are the cc-rs-emitted
    // weak symbol markers zenoh-pico uses to gate link backends —
    // they must be present (defined or external-ref) in the archive
    // for the link backend to compile in.
    let nm = Command::new("nm")
        .arg("--defined-only")
        .arg(&archive)
        .output()
        .expect("nm failed to spawn");
    if !nm.status.success() {
        panic!(
            "nm exited non-zero on {}. Stderr:\n{}",
            archive.display(),
            String::from_utf8_lossy(&nm.stderr)
        );
    }
    let nm_out = String::from_utf8_lossy(&nm.stdout);

    // Phase 134/136 contract — every transport's `*_open` /
    // `*_close` / `*_read` / `*_send` quartet must be defined in
    // the archive when `link.<transport>` is on per the POSIX
    // policy. Picking the open symbol per transport is enough to
    // catch the "transport silently disabled" regression — Phase
    // 134's symbol-parity gate covers the wrapper/impl matching
    // case so this test stays narrow.
    for sym in [
        "_z_open_tcp",
        "_z_open_udp_unicast",
        "_z_open_udp_multicast",
    ] {
        assert!(
            nm_out.contains(sym),
            "expected `{}` defined in {} (POSIX link.tcp / link.udp_*\
             policy is on), but it's missing. nm output sample:\n{}",
            sym,
            archive.display(),
            // Trim to a manageable error message — first 40 lines
            // of nm output is plenty for grep-style diagnosis.
            nm_out.lines().take(40).collect::<Vec<_>>().join("\n")
        );
    }
}
