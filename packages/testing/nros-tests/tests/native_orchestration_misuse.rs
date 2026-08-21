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
    nros_tests::fixtures::fixture_dir("orchestration_tiers_native")
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
fn launch_arm_resolves_the_bringup() {
    // phase-330 W7 — `nros::main!(launch = "<bringup>[:file]")` is the SUPPORTED
    // spelling: an entry names its INPUT and the build owns the model
    // (CLAUDE.md "SystemModels are BUILD ARTIFACTS").
    //
    // This test previously asserted the opposite — phase-296 R-code.1 had
    // REMOVED the arm, and the test guarded that removal. W7 brought it back and
    // the test was not retired with it, so it failed by succeeding: "expected the
    // launch-arm removal error, but check succeeded". A test that outlives the
    // rule it guards inverts into a guard against the CURRENT contract.
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
        out.status.success(),
        "`nros::main!(launch = …)` is the supported entry spelling (phase-330 W7) \
         and must compile.\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );
}
