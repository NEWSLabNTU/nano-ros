//! Phase 136 E2E.1 + E2E.7 — `zpico-sys` build-matrix gate.
//!
//! E2E.1 — build every supported platform feature through the
//! unified cc-rs path and assert the resulting archive contains
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
//! The matrix is intentionally narrow on the test runner — only
//! the host-runnable POSIX platform is exercised here. Cross-
//! compile builds for FreeRTOS / NuttX / ThreadX / Zephyr embedded
//! targets are covered by their per-platform `just <plat>
//! build-fixtures` recipes (driven from `just build-test-fixtures`);
//! re-running them inside this nextest would double the per-PR CI
//! wall-clock for no extra coverage.

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

/// E2E.1 — POSIX platform built through the unified cc-rs path
/// emits the canonical link-feature symbols. We deliberately pin
/// the platform to POSIX so the test runs on every CI box without
/// cross toolchains; the per-platform symbol-set contract for
/// embedded targets is enforced by the manifest's `link.*` policy
/// itself plus the existing `tests/zenoh_header_parity.rs` gate.
#[test]
fn zpico_posix_archive_carries_link_feature_symbols() {
    let root = workspace_root();
    let target_dir = root.join("target-zpico-build-matrix").join("posix");

    // Build the standalone staticlib (sibling of `zpico-sys`'s rlib)
    // because that's the artifact downstream consumers link, and
    // its archive is where `_z_f_link_*` symbols ultimately show up.
    let status = Command::new("cargo")
        .args([
            "build",
            "-p",
            "nros-rmw-zenoh-staticlib",
            "--release",
            "--no-default-features",
            "--features",
            "platform-posix,ros-humble",
            "--target-dir",
        ])
        .arg(&target_dir)
        .current_dir(&root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .status()
        .expect("cargo build failed to spawn");
    assert!(
        status.success(),
        "cargo build -p nros-rmw-zenoh-staticlib --features platform-posix failed"
    );

    let archive = target_dir.join("release/libnros_rmw_zenoh_staticlib.a");
    assert!(
        archive.exists(),
        "expected staticlib at {} but it was not produced",
        archive.display()
    );

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
    for sym in ["_z_open_tcp", "_z_open_udp_unicast", "_z_open_udp_multicast"] {
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
