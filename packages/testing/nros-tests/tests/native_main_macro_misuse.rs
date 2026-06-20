//! `nros::main!()` misuse diagnostics + rebuild-tracking.
//!
//! **These tests compile at run time — the documented exception to "No
//! compilation inside tests" (AGENTS.md / issue 0034).** A compile-*pass* check
//! moves to the build stage as a fixture (see `native_main_macro_forms.rs`),
//! but these cases cannot:
//!   * the three misuse cases must **fail to compile** with a specific
//!     diagnostic — you can't prebuild a build that must fail;
//!   * the rebuild case must run **two** checks across a file touch to prove the
//!     macro's `include_bytes!` stamp forces a re-check.
//!
//! They `cargo check` a staged copy of the `n9_workspace` template directly.
//! The `.config/nextest.toml` timeout-override keeps `native_main_macro_misuse`
//! (a cold check exceeds the 60s default) — this binary is intentionally on
//! that list; the converted positive forms are not.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn workspace_root() -> PathBuf {
    nros_tests::project_root()
}

fn fixture_src() -> PathBuf {
    workspace_root().join("packages/testing/nros-tests/fixtures/n9_workspace")
}

fn stage_fixture() -> (tempfile::TempDir, PathBuf) {
    let src = fixture_src();
    let dst = tempfile::tempdir().expect("tempdir");
    copy_tree(&src, dst.path()).expect("copy fixture");
    let root_str = workspace_root()
        .to_str()
        .expect("workspace root utf-8")
        .to_string();
    rewrite_placeholders(dst.path(), &root_str).expect("rewrite placeholders");
    let root = dst.path().to_path_buf();
    (dst, root)
}

fn copy_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_tree(&from, &to)?;
        } else if ty.is_file() {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

fn rewrite_placeholders(root: &Path, replacement: &str) -> std::io::Result<()> {
    for entry in walk(root)? {
        if !entry.is_file() {
            continue;
        }
        let Ok(text) = fs::read_to_string(&entry) else {
            continue;
        };
        if !text.contains("@NANO_ROS_ROOT@") {
            continue;
        }
        fs::write(&entry, text.replace("@NANO_ROS_ROOT@", replacement))?;
    }
    Ok(())
}

fn walk(root: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(p) = stack.pop() {
        if p.is_dir() {
            for e in fs::read_dir(&p)? {
                stack.push(e?.path());
            }
        } else {
            out.push(p);
        }
    }
    Ok(out)
}

fn check_demo_entry(root: &Path) -> std::process::Output {
    Command::new("cargo")
        .args(["check", "-p", "demo_entry", "--manifest-path"])
        .arg(root.join("Cargo.toml"))
        .output()
        .expect("spawn cargo check")
}

#[test]
fn custom_tasks_on_owned_spin_emits_error() {
    // `custom_tasks = [...]` is RTIC-only; the `demo_entry` fixture deploys
    // native (OwnedSpin), so it must fail with the documented diagnostic.
    let (_g, root) = stage_fixture();
    fs::write(
        root.join("src/demo_entry/src/main.rs"),
        "nros::main!(custom_tasks = [adc_sample, ui_redraw]);\n",
    )
    .expect("write main.rs");
    let out = check_demo_entry(&root);
    assert!(
        !out.status.success(),
        "expected `cargo check` to fail when `custom_tasks` is used outside RTIC.\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("custom_tasks") && stderr.contains("only valid for the RTIC framework"),
        "expected diagnostic to flag RTIC-only restriction, stderr:\n{stderr}",
    );
}

#[test]
fn custom_tasks_empty_on_owned_spin_still_errors() {
    // Empty list `custom_tasks = []` is also a misuse outside RTIC.
    let (_g, root) = stage_fixture();
    fs::write(
        root.join("src/demo_entry/src/main.rs"),
        "nros::main!(custom_tasks = []);\n",
    )
    .expect("write main.rs");
    let out = check_demo_entry(&root);
    assert!(
        !out.status.success(),
        "expected `cargo check` to fail for `custom_tasks = []` on OwnedSpin.\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("custom_tasks") && stderr.contains("only valid for the RTIC framework"),
        "expected diagnostic to flag RTIC-only restriction, stderr:\n{stderr}",
    );
}

#[test]
fn unknown_board_emits_compile_error() {
    let (_g, root) = stage_fixture();
    // Override Cargo.toml's `deploy = "native"` to an unknown board.
    let cargo_toml = root.join("src/demo_entry/Cargo.toml");
    let raw = fs::read_to_string(&cargo_toml).expect("read demo_entry Cargo.toml");
    let bad = raw.replace("deploy = \"native\"", "deploy = \"frobnicator\"");
    assert_ne!(raw, bad, "expected `deploy = \"native\"` line in fixture");
    fs::write(&cargo_toml, bad).expect("write bad Cargo.toml");

    fs::write(root.join("src/demo_entry/src/main.rs"), "nros::main!();\n").expect("write main.rs");
    let out = check_demo_entry(&root);
    assert!(
        !out.status.success(),
        "expected `cargo check` to fail on unknown board `frobnicator`.\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unknown board"),
        "expected diagnostic mentioning `unknown board`, stderr:\n{stderr}",
    );
}

#[test]
fn rebuilds_on_launch_xml_touch() {
    let (_g, root) = stage_fixture();
    fs::write(
        root.join("src/demo_entry/src/main.rs"),
        "nros::main!(launch = \"demo_bringup\");\n",
    )
    .expect("write main.rs");
    let out = check_demo_entry(&root);
    assert!(
        out.status.success(),
        "initial cargo check failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    // Touch the launch.xml — the macro's `include_bytes!` stamp should force a
    // re-check. Sleep past cargo's fingerprint mtime resolution, then rewrite.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let launch_xml = root.join("src/demo_bringup/launch/system.launch.xml");
    let body = fs::read_to_string(&launch_xml).expect("read launch.xml");
    fs::write(&launch_xml, &body).expect("rewrite launch.xml");

    let out2 = check_demo_entry(&root);
    assert!(
        out2.status.success(),
        "second cargo check (post-touch) failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out2.stdout),
        String::from_utf8_lossy(&out2.stderr),
    );
    let stderr2 = String::from_utf8_lossy(&out2.stderr);
    assert!(
        stderr2.contains("Checking demo_entry") || stderr2.contains("Compiling demo_entry"),
        "expected demo_entry to be re-checked after launch.xml touch, stderr:\n{stderr2}",
    );
}
