//! Phase 228.G — multi-tier `nros::main!()` emit round-trip.
//!
//! Stages the `orchestration_tiers_native` fixture into a tempdir, rewrites
//! `@NANO_ROS_ROOT@` to absolute `path =` deps, and `cargo check`s the Entry
//! pkg. Because `system.toml` declares `[tiers.high]` + `[tiers.low]`, the
//! `nros::main!()` macro resolves a 2-tier table and emits
//! `<NativeBoard>::run_tiers(TIERS, run_plan)` (RFC-0032 §5). The check passing
//! proves that multi-tier emit produces valid code; the negative test proves the
//! instance-identity guard (RFC-0032 §7) fires.
//!
//! Run with: `cargo test -p nros-tests --test orchestration_tiers_native`

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn workspace_root() -> PathBuf {
    nros_tests::project_root()
}

fn fixture_src() -> PathBuf {
    workspace_root().join("packages/testing/nros-tests/fixtures/orchestration_tiers_native")
}

fn stage_fixture() -> (tempfile::TempDir, PathBuf) {
    let src = fixture_src();
    let dst = tempfile::tempdir().expect("tempdir");
    copy_tree(&src, dst.path()).expect("copy fixture");
    let root_str = workspace_root().to_str().expect("utf-8").to_string();
    rewrite_placeholders(dst.path(), &root_str).expect("rewrite placeholders");
    let root = dst.path().to_path_buf();
    (dst, root)
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

fn rewrite_placeholders(root: &Path, replacement: &str) -> std::io::Result<()> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(p) = stack.pop() {
        if p.is_dir() {
            for e in fs::read_dir(&p)? {
                stack.push(e?.path());
            }
        } else if let Ok(text) = fs::read_to_string(&p) {
            if text.contains("@NANO_ROS_ROOT@") {
                fs::write(&p, text.replace("@NANO_ROS_ROOT@", replacement))?;
            }
        }
    }
    Ok(())
}

fn cargo_check(root: &Path) -> std::process::Output {
    Command::new("cargo")
        .args(["check", "-p", "demo_entry", "--manifest-path"])
        .arg(root.join("Cargo.toml"))
        .output()
        .expect("spawn cargo check")
}

#[test]
fn multi_tier_main_macro_emits_run_tiers_and_compiles() {
    let (_g, root) = stage_fixture();
    let out = cargo_check(&root);
    assert!(
        out.status.success(),
        "multi-tier `nros::main!()` emit failed to compile.\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}

#[test]
fn instance_identity_mismatch_is_a_compile_error() {
    // RFC-0032 §7: a launch node that carries callback groups but does not name
    // a `[[component]]` is a hard error. Rename one launch node away from its
    // component name and assert the macro rejects it.
    let (_g, root) = stage_fixture();
    let launch = root.join("src/demo_bringup/launch/system.launch.xml");
    let body = fs::read_to_string(&launch).expect("read launch.xml");
    let bad = body.replace("name=\"control_node\"", "name=\"control_typo\"");
    assert_ne!(body, bad, "expected control_node in fixture launch");
    fs::write(&launch, bad).expect("write launch.xml");

    let out = cargo_check(&root);
    assert!(
        !out.status.success(),
        "expected a compile error for the instance-identity mismatch, but check succeeded.\n\
         stderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("not a `[[component]]`") || stderr.contains("must match a `system.toml`"),
        "expected the instance-identity diagnostic, stderr:\n{stderr}",
    );
}
