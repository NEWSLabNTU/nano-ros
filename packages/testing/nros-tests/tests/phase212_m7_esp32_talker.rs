//! Phase 212.M.7 — `examples/esp32/rust/talker/` builds via `idf.py`.
//!
//! Stages the example into a tempdir and invokes
//! `idf.py -B <build> set-target esp32c3 && idf.py -B <build> build`.
//! Skips cleanly when `nros` CLI, `$IDF_PATH`, or `idf.py` are missing.
//!
//! Sibling to `phase212_h5_esp_idf.rs`. Uses the example's
//! `NANO_ROS_ROOT` env override so the staged copy can still find
//! the real `integrations/nano-ros/` shell under the worktree root.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn workspace_root() -> PathBuf {
    nros_tests::project_root()
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

fn stage_example(rel: &str) -> (tempfile::TempDir, PathBuf) {
    let src = workspace_root().join(rel);
    let dst = tempfile::tempdir().expect("tempdir");
    copy_tree(&src, dst.path()).expect("copy example");
    let root = dst.path().to_path_buf();
    (dst, root)
}

fn run_idf_build(example_rel: &str, project_name: &str) {
    if !nros_tests::require_nros_cli() {
        nros_tests::skip!("nros CLI not found");
    }
    if !nros_tests::esp32::require_esp_idf() {
        nros_tests::skip!("ESP-IDF not reachable ($IDF_PATH + idf.py)");
    }

    let (_guard, example_dir) = stage_example(example_rel);
    let build_dir = example_dir.join("build");

    let set_target = Command::new("idf.py")
        .arg("-B")
        .arg(&build_dir)
        .arg("set-target")
        .arg("esp32c3")
        .env("NANO_ROS_ROOT", workspace_root())
        .current_dir(&example_dir)
        .output()
        .expect("spawn idf.py set-target");
    if !set_target.status.success() {
        nros_tests::skip!(
            "idf.py set-target failed (likely tools venv not sourced):\n\
             stdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&set_target.stdout),
            String::from_utf8_lossy(&set_target.stderr)
        );
    }

    let build = Command::new("idf.py")
        .arg("-B")
        .arg(&build_dir)
        .arg("build")
        .env("NANO_ROS_ROOT", workspace_root())
        .current_dir(&example_dir)
        .output()
        .expect("spawn idf.py build");
    assert!(
        build.status.success(),
        "idf.py build failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );

    let elf = build_dir.join(format!("{}.elf", project_name));
    assert!(elf.is_file(), "missing ELF at {}", elf.display());
}

#[test]
fn esp32_talker_builds_via_idf_py() {
    run_idf_build("examples/esp32/rust/talker", "esp32_bsp_talker");
}
