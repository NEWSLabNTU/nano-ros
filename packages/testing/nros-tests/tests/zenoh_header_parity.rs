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

fn find_out_dir_header(root: &Path) -> Option<std::path::PathBuf> {
    let glob_root = root.join("target");
    if !glob_root.exists() {
        return None;
    }
    let mut stack = vec![glob_root];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = entry.file_name();
                let s = name.to_string_lossy();
                if s.starts_with("zpico-sys-") {
                    let candidate = path.join("out/zenoh-config/zenoh_generic_config.h");
                    if candidate.exists() {
                        return Some(candidate);
                    }
                }
                if !path.is_symlink() {
                    stack.push(path);
                }
            }
        }
    }
    None
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
