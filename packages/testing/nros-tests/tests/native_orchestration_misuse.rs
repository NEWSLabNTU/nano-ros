//! Multi-tier instance-identity guard (RFC-0032 §7) — a compile error.
//!
//! **Compiles at run time — the documented exception to "No compilation inside
//! tests" (AGENTS.md / issue 0034):** a compile-*fail* diagnostic can't be
//! prebuilt as a passing fixture. The test stages the
//! `orchestration_tiers_native` template, renames a launch node away from its
//! `[[component]]`, and asserts `cargo check` fails with the instance-identity
//! diagnostic. Kept on the `.config/nextest.toml` timeout-override (a cold check
//! exceeds the 60s default).

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn fixture_src() -> PathBuf {
    nros_tests::project_root()
        .join("packages/testing/nros-tests/fixtures/orchestration_tiers_native")
}

fn stage_fixture() -> (tempfile::TempDir, PathBuf) {
    let src = fixture_src();
    let dst = tempfile::tempdir().expect("tempdir");
    copy_tree(&src, dst.path()).expect("copy fixture");
    let root_str = nros_tests::project_root()
        .to_str()
        .expect("utf-8")
        .to_string();
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
        } else if let Ok(text) = fs::read_to_string(&p)
            && text.contains("@NANO_ROS_ROOT@")
        {
            fs::write(&p, text.replace("@NANO_ROS_ROOT@", replacement))?;
        }
    }
    Ok(())
}

#[test]
fn launch_arm_is_a_removal_error() {
    // R-code.1 — the `launch = …` macro arm is REMOVED; using it must fail
    // with the actionable migrate-to-model diagnostic (the launch↔system.toml
    // identity check retired with the arm; the model form's integrity is the
    // resolve-time checker's job).
    let (_g, root) = stage_fixture();
    fs::write(
        root.join("src/demo_entry/src/main.rs"),
        "nros::main!(launch = \"demo_bringup\");\n",
    )
    .expect("write launch-arm main.rs");

    let out = Command::new("cargo")
        .args(["check", "-p", "demo_entry", "--manifest-path"])
        .arg(root.join("Cargo.toml"))
        .output()
        .expect("spawn cargo check");
    assert!(
        !out.status.success(),
        "expected the launch-arm removal error, but check succeeded.\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("removed") && stderr.contains("play_launch resolve"),
        "expected the removal diagnostic naming the resolve command, stderr:\n{stderr}",
    );
}
