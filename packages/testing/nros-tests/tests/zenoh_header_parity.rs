//! Phase 134.6 E2E.4 — canonical header content gate.
//!
//! The canonical `zenoh_generic_config.h` lives at
//! `<OUT_DIR>/zenoh-config/zenoh_generic_config.h`, generated once
//! per build by `zpico-sys`'s `generate_config_header`. Pre-134
//! the CMake POSIX path bypassed it entirely (upstream's
//! `configure_file` baked CMake cache values into
//! `zenoh-pico/include/zenoh-pico/config.h` directly). Post-134
//! `ZENOH_GENERIC` routes every compile unit through our header;
//! this test asserts the header carries the values the canonical
//! `LinkFeatures::apply(LinkPolicy::posix())` chain produces, so
//! we catch any future change that silently flips a flag.
//!
//! Test FAILS (not skips) if:
//!   - the header is absent (run `cargo build -p
//!     nros-rmw-zenoh-staticlib --features platform-posix` first),
//!     OR
//!   - any `Z_FEATURE_LINK_*` value drifts from the contract.

use std::{fs, path::Path};

fn workspace_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("workspace root above CARGO_MANIFEST_DIR")
        .to_path_buf()
}

/// Phase 150.E — return the most-recently-modified header from
/// `<workspace>/target/{debug,release}/build/zpico-sys-*/out/...`.
///
/// Restricting to the workspace-default native target dir is
/// load-bearing: a cross-target build (e.g. `target/riscv64gc-…/`
/// from a recent `just threadx_riscv64 build-fixtures`) produces
/// a `zpico-sys-*` build dir for ThreadX, which goes through
/// `LinkPolicy::threadx()` (serial/udp_unicast/udp_multicast forced
/// off — Phase 146.2). Picking that header by accident would make
/// every POSIX-policy assertion fail. The native POSIX path always
/// lives under `target/{debug,release}/`, never under a
/// `target/<triple>/` sub-directory.
///
/// Pick the most-recent across `debug/` and `release/` so the test
/// reflects the latest POSIX build regardless of profile.
fn find_out_dir_header(root: &Path) -> Option<std::path::PathBuf> {
    let mut newest: Option<(std::time::SystemTime, std::path::PathBuf)> = None;
    for profile in ["debug", "release"] {
        let build_dir = root.join("target").join(profile).join("build");
        let Ok(entries) = fs::read_dir(&build_dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let s = name.to_string_lossy();
            if !s.starts_with("zpico-sys-") {
                continue;
            }
            let candidate = path.join("out/zenoh-config/zenoh_generic_config.h");
            let Ok(meta) = fs::metadata(&candidate) else {
                continue;
            };
            let Ok(mtime) = meta.modified() else {
                continue;
            };
            if newest
                .as_ref()
                .is_none_or(|(prev_mtime, _)| mtime > *prev_mtime)
            {
                newest = Some((mtime, candidate));
            }
        }
    }
    newest.map(|(_, p)| p)
}

fn parse_define(header: &str, key: &str) -> Option<String> {
    for line in header.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("#define ") {
            let mut parts = rest.splitn(2, char::is_whitespace);
            let name = parts.next().unwrap_or("");
            if name == key {
                return parts.next().map(|s| s.trim().to_string());
            }
        }
    }
    None
}

#[test]
fn posix_canonical_header_matches_link_policy() {
    let root = workspace_root();
    let header_path = find_out_dir_header(&root).expect(
        "zenoh_generic_config.h not found under any \
         target/**/zpico-sys-*/out/zenoh-config/. \
         Run `cargo build -p nros-rmw-zenoh-staticlib \
         --features platform-posix` first (Phase 134 contract \
         presumes the canonical header has been generated).",
    );

    let text = fs::read_to_string(&header_path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", header_path.display()));

    // `LinkPolicy::posix()` today is pure passthrough +
    // `LinkFeatures::from_env()` forces tcp/udp_unicast/udp_multicast/serial
    // = true. raweth/tls/ivc/custom default to false unless their
    // Cargo features are set. The cargo invocation above pulls in
    // `platform-posix` only (no link-* feature opt-ins), so the
    // expected values are:
    let expected: &[(&str, &str)] = &[
        ("Z_FEATURE_LINK_TCP", "1"),
        ("Z_FEATURE_LINK_UDP_UNICAST", "1"),
        ("Z_FEATURE_LINK_UDP_MULTICAST", "1"),
        ("Z_FEATURE_LINK_SERIAL", "1"),
        ("Z_FEATURE_LINK_BLUETOOTH", "0"),
        ("Z_FEATURE_LINK_WS", "0"),
        ("Z_FEATURE_LINK_SERIAL_USB", "0"),
        ("Z_FEATURE_LINK_IVC", "0"),
        ("Z_FEATURE_LINK_CUSTOM", "0"),
        ("Z_FEATURE_LINK_TLS", "0"),
        ("Z_FEATURE_RAWETH_TRANSPORT", "0"),
        // Phase 134.4 — INTEREST + MATCHING must be on for proper
        // cross-network routing (Zephyr-TAP vs native-loopback).
        ("Z_FEATURE_INTEREST", "1"),
        ("Z_FEATURE_MATCHING", "1"),
    ];

    let mut mismatches = Vec::new();
    for (key, want) in expected {
        match parse_define(&text, key) {
            Some(got) if got == *want => {}
            Some(got) => mismatches.push(format!("  {key}: header={got} expected={want}")),
            None => mismatches.push(format!("  {key}: MISSING (expected {want})")),
        }
    }

    if !mismatches.is_empty() {
        panic!(
            "Phase 134 canonical-header content gate FAILED.\n\
             Header: {}\n\
             Drift from LinkPolicy::posix() contract:\n{}",
            header_path.display(),
            mismatches.join("\n"),
        );
    }
}
