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
fn instance_identity_mismatch_is_a_compile_error() {
    let (_g, root) = stage_fixture();
    // R-code.1 stage 1: the staged fixture entry is on the model arm now, so
    // pin THIS test to the launch arm it exercises (the launch-XML↔system.toml
    // identity check). Stage 2 (launch-arm removal) either ports the check to
    // the model arm (orphaned execution.bindings should fail loud) or retires
    // this test with the arm.
    fs::write(
        root.join("src/demo_entry/src/main.rs"),
        "nros::main!(launch = \"demo_bringup\");\n",
    )
    .expect("write launch-arm main.rs");
    let launch = root.join("src/demo_bringup/launch/system.launch.xml");
    let body = fs::read_to_string(&launch).expect("read launch.xml");
    let bad = body.replace("name=\"control_node\"", "name=\"control_typo\"");
    assert_ne!(body, bad, "expected control_node in fixture launch");
    fs::write(&launch, bad).expect("write launch.xml");

    let out = Command::new("cargo")
        .args(["check", "-p", "demo_entry", "--manifest-path"])
        .arg(root.join("Cargo.toml"))
        .output()
        .expect("spawn cargo check");
    assert!(
        !out.status.success(),
        "expected a compile error for the instance-identity mismatch, but check succeeded.\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("not a `[[component]]`") || stderr.contains("must match a `system.toml`"),
        "expected the instance-identity diagnostic, stderr:\n{stderr}",
    );
}
