//! Phase 136 E2E.3 — zenoh-pico source-list drift gate.
//!
//! `zpico-sys/build.rs` panics at build time if any
//! `[platform.*] include` root in `zenoh_platforms.toml` no longer
//! resolves to a real directory under `zenoh-pico/src/`. The check
//! is the structural firewall against silent stale-source bugs
//! when upstream zenoh-pico bumps rename `system/<plat>/` dirs.
//!
//! This test guards the gate itself: it copies the manifest to a
//! sandbox, corrupts one `include` entry, drives `cargo build
//! -p zpico-sys` against the sandboxed manifest, and asserts the
//! build script panic surfaces with the documented diagnostic. A
//! second run restores the manifest and asserts the build passes.
//!
//! Driven by the `ZPICO_PLATFORMS_TOML` env var (override hook
//! introduced alongside 136.1). If the override is not honoured —
//! e.g. someone deletes the env read in a future refactor — the
//! test fails because the corrupted sandbox manifest is silently
//! ignored. That's the second invariant: the override hook itself
//! must keep working.

use std::{
    fs,
    path::PathBuf,
    process::{Command, Stdio},
};

fn workspace_root() -> PathBuf {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .ancestors()
        .nth(3)
        .expect("workspace root above CARGO_MANIFEST_DIR")
        .to_path_buf()
}

fn canonical_manifest_path() -> PathBuf {
    workspace_root().join("packages/zpico/zpico-sys/zenoh_platforms.toml")
}

/// Run `cargo build -p zpico-sys` with `ZPICO_PLATFORMS_TOML`
/// pointed at `manifest_path`. Returns the combined stdout+stderr
/// output and the exit status. Build is done in a dedicated
/// `target-zpico-drift-gate/` dir so it doesn't poison other
/// concurrent builds.
fn run_build(manifest_path: &std::path::Path) -> (String, std::process::ExitStatus) {
    let root = workspace_root();
    let target_dir = root.join("target-zpico-drift-gate");

    let output = Command::new("cargo")
        .args([
            "build",
            "-p",
            "zpico-sys",
            "--no-default-features",
            "--features",
            // `zpico-sys` exposes per-platform features as bare
            // names (`posix`, `zephyr`, `freertos`, ...), unlike
            // the umbrella `nros` / `nros-rmw-zenoh` crates that
            // prefix them with `platform-`. Phase 136.7 E2E.3.
            "posix,platform-aliases",
            "--target-dir",
        ])
        .arg(&target_dir)
        .env("ZPICO_PLATFORMS_TOML", manifest_path)
        // Phase 134: keep CARGO_TARGET_DIR / RUSTUP_TOOLCHAIN intact
        // from the parent test process so the same toolchain that
        // built the test binary builds the sandbox crate.
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("cargo build failed to spawn");

    let mut combined = String::from_utf8_lossy(&output.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    (combined, output.status)
}

#[test]
fn zpico_drift_gate_fires_on_corrupted_include() {
    let root = workspace_root();
    let canonical = canonical_manifest_path();
    if !canonical.exists() {
        panic!(
            "[SKIPPED] zenoh_platforms.toml not present at {} — \
             zpico-sys layout drifted from Phase 136",
            canonical.display()
        );
    }
    // Skip if the override hook isn't wired yet — pre-136 builds
    // hard-coded the manifest path and this test can't function.
    let probe = std::env::temp_dir().join("zpico-drift-gate-probe.toml");
    fs::copy(&canonical, &probe).expect("copy canonical manifest");
    let (out, status) = run_build(&probe);
    if !status.success() {
        // The override hook may not exist yet — only fail the test
        // if the failure is specifically about the override not
        // working, otherwise re-raise.
        if !out.contains("ZPICO_PLATFORMS_TOML") && !out.contains("zenoh_platforms.toml") {
            panic!(
                "[SKIPPED] cargo build failed with the canonical manifest in the sandbox; \
                 either the ZPICO_PLATFORMS_TOML override hook is not implemented yet, or \
                 the build environment is broken. Output:\n{out}"
            );
        }
    }
    let _ = fs::remove_file(&probe);

    let canonical_body = fs::read_to_string(&canonical).expect("read canonical manifest");
    if !canonical_body.contains("system/unix") {
        panic!(
            "[SKIPPED] manifest doesn't carry the expected `system/unix` include — \
             test fixture assumption drifted; update the corruption pattern"
        );
    }
    let corrupted_body = canonical_body.replace(
        "system/unix",
        // A path that obviously doesn't exist under zenoh-pico/src/.
        // Keep it inside `system/` so the gate's path-resolution
        // logic actually traverses to it and fails.
        "system/_zpico_drift_gate_sentinel_does_not_exist",
    );

    let sandbox = root.join("target-zpico-drift-gate").join("manifest");
    fs::create_dir_all(&sandbox).expect("create sandbox dir");
    let corrupted_path = sandbox.join("zenoh_platforms.toml");
    fs::write(&corrupted_path, corrupted_body).expect("write corrupted manifest");

    let (out, status) = run_build(&corrupted_path);
    assert!(
        !status.success(),
        "expected cargo build to fail with the corrupted manifest, \
         but it succeeded. Output:\n{out}"
    );
    assert!(
        out.contains("_zpico_drift_gate_sentinel_does_not_exist")
            || out.contains("zenoh-pico source list drift")
            || out.contains("does not exist")
            || out.contains("not found"),
        "expected the drift diagnostic to name the corrupted path or carry the documented \
         drift message, but neither appears. Output:\n{out}"
    );

    // Round-trip: restore the manifest and confirm the build is happy
    // again, so future runs of this test stay deterministic.
    let restored_path = sandbox.join("zenoh_platforms_restored.toml");
    fs::write(&restored_path, &canonical_body).expect("write restored manifest");
    let (out, status) = run_build(&restored_path);
    assert!(
        status.success(),
        "restored manifest should build cleanly, but it failed. Output:\n{out}"
    );
}
