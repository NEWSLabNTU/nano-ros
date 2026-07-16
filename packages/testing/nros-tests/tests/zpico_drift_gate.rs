//! Phase 136 E2E.3 / phase-290 — zenoh-pico source-list drift gate.
//!
//! `zpico-sys/build.rs` panics at build time if any
//! `[build.zenoh] include` root in a platform package's
//! `nros-platform.toml` no longer resolves to a real directory under
//! `zenoh-pico/src/`. The check is the structural firewall against
//! silent stale-source bugs when upstream zenoh-pico bumps rename
//! `system/<plat>/` dirs.
//!
//! This test guards the gate itself: it copies the per-platform config
//! tree (`packages/platforms/*/nros-platform.toml`, RFC-0049) to a
//! sandbox, corrupts the posix file's `include` entry, drives
//! `cargo build -p zpico-sys` against the sandboxed tree via
//! `NROS_PLATFORMS_DIR`, and asserts the build-script panic surfaces
//! with the documented diagnostic. A second run against the pristine
//! sandbox asserts the build passes.
//!
//! Second invariant: the `NROS_PLATFORMS_DIR` override hook itself must
//! keep working — if a future refactor drops the env read, the corrupted
//! sandbox is silently ignored and the first assertion fails.

use std::{
    fs,
    path::{Path, PathBuf},
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

fn canonical_platforms_root() -> PathBuf {
    workspace_root().join("packages/platforms")
}

/// Copy every `<root>/*/nros-platform.toml` into `dst` preserving the
/// per-directory layout the loader expects.
fn copy_tree(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).expect("create sandbox tree root");
    for entry in fs::read_dir(src).expect("read platforms root") {
        let entry = entry.expect("dir entry");
        let file = entry.path().join("nros-platform.toml");
        if !file.is_file() {
            continue;
        }
        let name = entry.file_name();
        let ddir = dst.join(&name);
        fs::create_dir_all(&ddir).expect("create sandbox platform dir");
        fs::copy(&file, ddir.join("nros-platform.toml")).expect("copy platform toml");
    }
}

/// Run `cargo build -p zpico-sys` with `NROS_PLATFORMS_DIR` pointed at
/// `platforms_dir`. Returns combined stdout+stderr and the exit status.
/// Dedicated `target-zpico-drift-gate/` dir so it doesn't poison other
/// concurrent builds.
fn run_build(platforms_dir: &Path) -> (String, std::process::ExitStatus) {
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
        .env("NROS_PLATFORMS_DIR", platforms_dir)
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
    let canonical_root = canonical_platforms_root();
    let posix_toml = canonical_root.join("posix/nros-platform.toml");
    if !posix_toml.exists() {
        panic!(
            "[SKIPPED] {} not present — the phase-290 per-platform config \
             layout drifted",
            posix_toml.display()
        );
    }

    let posix_body = fs::read_to_string(&posix_toml).expect("read posix platform toml");
    if !posix_body.contains("system/unix") {
        panic!(
            "[SKIPPED] posix nros-platform.toml doesn't carry the expected \
             `system/unix` include — test fixture assumption drifted; update \
             the corruption pattern"
        );
    }

    // Sandbox 1 — pristine copy. Also proves the NROS_PLATFORMS_DIR
    // override hook is honoured (a broken hook fails the corrupted run
    // below instead).
    let sandbox_root = root.join("target-zpico-drift-gate").join("platforms");
    let pristine = sandbox_root.join("pristine");
    let _ = fs::remove_dir_all(&sandbox_root);
    copy_tree(&canonical_root, &pristine);

    // Sandbox 2 — corrupted posix include.
    let corrupted = sandbox_root.join("corrupted");
    copy_tree(&canonical_root, &corrupted);
    let corrupted_body = posix_body.replace(
        "system/unix",
        // A path that obviously doesn't exist under zenoh-pico/src/.
        // Keep it inside `system/` so the gate's path-resolution
        // logic actually traverses to it and fails.
        "system/_zpico_drift_gate_sentinel_does_not_exist",
    );
    fs::write(corrupted.join("posix/nros-platform.toml"), corrupted_body)
        .expect("write corrupted platform toml");

    let (out, status) = run_build(&corrupted);
    assert!(
        !status.success(),
        "expected cargo build to fail with the corrupted platform tree, \
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

    // Round-trip: the pristine sandbox builds cleanly, so future runs of
    // this test stay deterministic.
    let (out, status) = run_build(&pristine);
    assert!(
        status.success(),
        "pristine platform tree should build cleanly, but it failed. Output:\n{out}"
    );
}
