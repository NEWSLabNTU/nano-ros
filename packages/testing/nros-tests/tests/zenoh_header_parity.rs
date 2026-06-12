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
//! Header discovery (Phase 150.E rev3):
//!
//! 1. `NROS_TESTS_ZENOH_HEADER` — explicit absolute path, wins.
//!    Use this in CI / out-of-tree consumers (e.g. point at a CMake
//!    build's `<build>/_deps/.../zenoh_generic_config.h`).
//! 2. `<workspace>/target-zenoh-fixture-posix/release/build/
//!    zpico-sys-*/out/zenoh-config/zenoh_generic_config.h` —
//!    deterministic fixture built by `just build-zenoh-posix-fixture`
//!    (pulled in by `just build-test-fixtures`). Only ONE
//!    `zpico-sys-<hash>` lives in this target-dir because the
//!    dir only ever has `nros-rmw-zenoh-staticlib --features
//!    platform-posix` (release) built into it; the wildcard is safe.
//!
//! Test FAILS (not skips) if neither source produces a header OR
//! any `Z_FEATURE_LINK_*` value drifts from the contract. Failure
//! mode (1) points the user at `just build-test-fixtures`; failure
//! mode (2) is the actual Phase 134 regression class.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("workspace root above CARGO_MANIFEST_DIR")
        .to_path_buf()
}

/// Phase 150.E rev2 — deterministic + overridable.
fn resolve_header_path() -> Result<PathBuf, String> {
    if let Some(explicit) = env::var_os("NROS_TESTS_ZENOH_HEADER") {
        let p = PathBuf::from(explicit);
        if p.is_file() {
            return Ok(p);
        }
        return Err(format!(
            "NROS_TESTS_ZENOH_HEADER points at {} but the file is missing",
            p.display(),
        ));
    }

    let root = workspace_root();
    // Fixture is built with `--release` so the host-native subdir is
    // `release/`. Allowing `debug/` as a secondary pick keeps the
    // contract honest for a future caller that drops `--release`;
    // both are deterministic because this --target-dir houses
    // exactly one feature set (`platform-posix`).
    for profile in ["release", "debug"] {
        let build_dir = root
            .join("target-zenoh-fixture-posix")
            .join(profile)
            .join("build");
        let Ok(entries) = fs::read_dir(&build_dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            if !name.to_string_lossy().starts_with("zpico-sys-") {
                continue;
            }
            let candidate = entry.path().join("out/zenoh-config/zenoh_generic_config.h");
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    Err(format!(
        "Phase 150.E fixture not built. Run:\n  \
         just build-zenoh-posix-fixture\n\
         (or `just build-test-fixtures` which pulls it in).\n\
         Override path via NROS_TESTS_ZENOH_HEADER=<abs/path/to/header>.\n\
         Workspace root searched: {}",
        root.display(),
    ))
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
    // Issue #34 — the zenoh-posix fixture (`target-zenoh-fixture-posix/`) is
    // built by `just build-zenoh-posix-fixture` / `build-test-fixtures`, which
    // the light host-integration lane does NOT run (it builds only the core
    // rust + workspace fixtures). Skip cleanly there (NROS_FIXTURES_OPTIONAL set)
    // rather than hard-failing on the missing artifact; the full `test-all` tier
    // (var unset) still fails loudly so a real header-drift regression surfaces.
    let header_path = match resolve_header_path() {
        Ok(p) => p,
        Err(e) if std::env::var_os("NROS_FIXTURES_OPTIONAL").is_some() => {
            nros_tests::skip!("zenoh-posix header fixture not built (light tier): {e}");
        }
        Err(e) => panic!("{e}"),
    };
    let text = fs::read_to_string(&header_path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", header_path.display()));

    // `LinkPolicy::posix()` today is pure passthrough +
    // `LinkFeatures::from_env()` forces tcp/udp_unicast/udp_multicast/serial
    // = true. raweth/tls/ivc/custom default to false unless their
    // Cargo features are set. The fixture recipe pulls in
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
