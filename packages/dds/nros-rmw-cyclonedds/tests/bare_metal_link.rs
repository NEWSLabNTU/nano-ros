//! Phase 212.K.7.8 — bare-metal link smoke for `nros-rmw-cyclonedds`.
//!
//! Spawns `cargo build -p nros-rmw-cyclonedds --no-default-features
//! --target thumbv7m-none-eabi` from a hosted test process and
//! asserts the rlib links cleanly. Proves the K.7.4-6 registry +
//! descriptor builder paths are genuinely `no_std` + alloc-free
//! under a `target_os = "none"` build (the `cs::RegistryMutex`
//! `#[cfg]` arm fires, `critical-section` is the lock backing,
//! `spin::Mutex` falls out by `--cfg` arm).
//!
//! Gating:
//!
//! * `#[cfg(feature = "std")]` — needs `std::process::Command`.
//! * `#[ignore]` — the build is heavy enough (~seconds, downloads
//!   nothing but invokes the compiler) that we don't want it in the
//!   default `cargo test` loop. Run via
//!   `cargo test -p nros-rmw-cyclonedds --no-default-features
//!   --features bridge-stub,std -- --ignored bare_metal`.
//! * Skips at runtime (without failing) when the
//!   `thumbv7m-none-eabi` target isn't installed — `rustup target
//!   list --installed` is the gate. CI is expected to install the
//!   target as part of `nros setup` / `just workspace setup`.

#![cfg(feature = "std")]

use std::{path::PathBuf, process::Command};

const TARGET: &str = "thumbv7m-none-eabi";

/// Probes `rustup target list --installed` for the bare-metal target.
/// Returns `true` iff `thumbv7m-none-eabi` is installed.
fn target_installed() -> bool {
    let out = match Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
    {
        Ok(o) => o,
        Err(_) => return false,
    };
    if !out.status.success() {
        return false;
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    stdout.lines().any(|l| l.trim() == TARGET)
}

/// Workspace root — climbs from `CARGO_MANIFEST_DIR` (the
/// `nros-rmw-cyclonedds` crate dir) up to the workspace root that
/// owns the unified target directory `cargo build` will populate.
fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = packages/dds/nros-rmw-cyclonedds
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    crate_dir
        .ancestors()
        .nth(3)
        .expect("workspace root above packages/dds/nros-rmw-cyclonedds")
        .to_path_buf()
}

#[test]
#[ignore = "heavy: invokes cargo build for thumbv7m-none-eabi"]
fn bare_metal_no_std_clean() {
    if !target_installed() {
        eprintln!("[SKIPPED] {TARGET} not installed. Run `rustup target add {TARGET}` to enable.");
        return;
    }

    let root = workspace_root();
    let out = Command::new(env!("CARGO"))
        .current_dir(&root)
        .args([
            "build",
            "-p",
            "nros-rmw-cyclonedds",
            "--no-default-features",
            "--target",
            TARGET,
        ])
        .output()
        .expect("cargo invocation must spawn");

    assert!(
        out.status.success(),
        "bare-metal build failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    // Verify the rlib actually landed in the expected target dir.
    let deps = root.join("target").join(TARGET).join("debug").join("deps");
    let mut found_rlib = false;
    if let Ok(entries) = std::fs::read_dir(&deps) {
        for e in entries.flatten() {
            let name = e.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("libnros_rmw_cyclonedds") && name.ends_with(".rlib") {
                found_rlib = true;
                break;
            }
        }
    }
    assert!(
        found_rlib,
        "expected libnros_rmw_cyclonedds*.rlib under {}",
        deps.display(),
    );
}

#[test]
#[ignore = "heavy: invokes cargo build for thumbv7m-none-eabi + nm scan"]
fn bare_metal_no_alloc_symbols() {
    // Runs the companion shell script `tests/alloc_free_audit.sh`,
    // which (1) builds the crate for `thumbv7m-none-eabi`, (2) runs
    // `nm` on the resulting rlib, (3) fails if any
    // `_ZN5alloc…` / `__rust_alloc…` symbols leaked in.
    if !target_installed() {
        eprintln!("[SKIPPED] {TARGET} not installed. Run `rustup target add {TARGET}` to enable.");
        return;
    }
    if Command::new("nm").arg("--version").output().is_err() {
        eprintln!("[SKIPPED] `nm` not on PATH; cannot audit symbols.");
        return;
    }

    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script = crate_dir.join("tests").join("alloc_free_audit.sh");
    assert!(
        script.exists(),
        "alloc_free_audit.sh missing at {}",
        script.display()
    );

    let out = Command::new("bash")
        .arg(&script)
        .current_dir(workspace_root())
        .output()
        .expect("alloc_free_audit.sh must spawn");

    assert!(
        out.status.success(),
        "alloc-free audit failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}
