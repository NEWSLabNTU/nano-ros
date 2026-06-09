//! Phase 228.D.2 — cross-LANGUAGE shared-state roundtrip.
//!
//! Stages the `shared_state_xlang` fixture (a `system.toml` with one
//! `[[shared_state]]` region + two components), bakes it with `nros
//! codegen-system`, then builds + runs the `consumer` crate that links BOTH
//! generated halves into one binary:
//!   - `nros_shared_state.rs` — the Rust accessors + `#[unsafe(no_mangle)]
//!     extern "C"` exports, consumed as a module;
//!   - `nros_shared_context.h` — the matching C typedef + decls, included by a
//!     C TU (`cross.c`) that calls the Rust-exported C-ABI accessors.
//!
//! The consumer asserts a Rust write is seen by C, a C write is seen by Rust,
//! and a guarded C `modify` is observed by Rust — i.e. both languages share the
//! ONE `LockedSharedRegion`. Seeing `xlang shared-state roundtrip OK` (exit 0)
//! is the proof. This is the cross-*language* consumer the single-language emit
//! tests (`emit_shared_state_rust_typed_accessors`) couldn't cover.
//!
//! Skips cleanly when the `nros` CLI or a C compiler is unavailable.
//!
//! Run with: `cargo test -p nros-tests --test orchestration_shared_state_xlang`

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn workspace_root() -> PathBuf {
    nros_tests::project_root()
}

fn fixture_src() -> PathBuf {
    workspace_root().join("packages/testing/nros-tests/fixtures/shared_state_xlang")
}

fn cc_available() -> bool {
    ["cc", "gcc", "clang"].iter().any(|cc| {
        Command::new(cc)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    })
}

fn copy_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&from, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

fn stage_fixture() -> (tempfile::TempDir, PathBuf) {
    let dst = tempfile::tempdir().expect("tempdir");
    copy_tree(&fixture_src(), dst.path()).expect("copy fixture");
    let root_str = workspace_root().to_str().expect("utf-8").to_string();
    let mut stack = vec![dst.path().to_path_buf()];
    while let Some(p) = stack.pop() {
        if p.is_dir() {
            for e in fs::read_dir(&p).expect("read_dir") {
                stack.push(e.expect("entry").path());
            }
        } else if let Ok(text) = fs::read_to_string(&p) {
            if text.contains("@NANO_ROS_ROOT@") {
                fs::write(&p, text.replace("@NANO_ROS_ROOT@", &root_str)).expect("rewrite");
            }
        }
    }
    let root = dst.path().to_path_buf();
    (dst, root)
}

#[test]
fn shared_state_region_is_shared_across_c_and_rust() {
    if !nros_tests::require_nros_cli() {
        nros_tests::skip!("nros CLI not found (run `just setup-cli` + `source ./activate.sh`)");
    }
    if !cc_available() {
        nros_tests::skip!("no C compiler (cc/gcc/clang) on PATH");
    }
    let nros = nros_tests::nros_cli_bin_path().expect("nros bin");
    let (_guard, root) = stage_fixture();

    // Bake the shared-state surface. Output lands under `<out>/nros-system/`.
    let out = root.join("build");
    let bake = Command::new(&nros)
        .args(["codegen-system", "--bringup", "demo_bringup", "--workspace"])
        .arg(&root)
        .arg("--out")
        .arg(&out)
        .output()
        .expect("spawn nros codegen-system");
    assert!(
        bake.status.success(),
        "nros codegen-system failed.\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&bake.stdout),
        String::from_utf8_lossy(&bake.stderr),
    );
    let bake_dir = out.join("nros-system");
    for f in ["nros_shared_state.rs", "nros_shared_context.h"] {
        assert!(
            bake_dir.join(f).is_file(),
            "bake did not emit {f} at {}",
            bake_dir.display()
        );
    }

    // Build + run the cross-language consumer against the bake.
    let consumer = root.join("consumer");
    let run = Command::new("cargo")
        .args(["run", "--quiet"])
        .current_dir(&consumer)
        .env("NROS_BAKE_DIR", &bake_dir)
        .output()
        .expect("spawn cargo run (consumer)");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(
        run.status.success(),
        "cross-language consumer failed (the generated region is not shared, or did not link).\n\
         output:\n{combined}",
    );
    assert!(
        combined.contains("xlang shared-state roundtrip OK"),
        "consumer ran but did not confirm the roundtrip.\noutput:\n{combined}",
    );
}
