//! Phase 212.H.5 — ESP-IDF 2-component bringup builds via `idf.py`.
//!
//! Stages `fixtures/multi_pkg_workspace_esp_idf/` into a tempdir, rewrites
//! `@NANO_ROS_ROOT@` in the IDF project's `CMakeLists.txt`, then invokes
//! `idf.py -B <build> build`. Skips cleanly when `nros` CLI, `$IDF_PATH`,
//! or `idf.py` are missing.
//!
//! Sibling to `phase212_d_workspace_metadata.rs`; uses the same
//! stage-tempdir + placeholder-rewrite pattern.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn workspace_root() -> PathBuf {
    nros_tests::project_root()
}

fn fixture(name: &str) -> PathBuf {
    workspace_root()
        .join("packages/testing/nros-tests/fixtures")
        .join(name)
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

/// Stage the fixture and rewrite the IDF app's CMakeLists.txt
/// `@NANO_ROS_ROOT@` placeholder.
fn stage_fixture() -> (tempfile::TempDir, PathBuf) {
    let src = fixture("multi_pkg_workspace_esp_idf");
    let dst = tempfile::tempdir().expect("tempdir");
    copy_tree(&src, dst.path()).expect("copy fixture");
    let top = dst.path().join("esp_idf_app/CMakeLists.txt");
    let rendered = fs::read_to_string(&top)
        .expect("read esp_idf_app/CMakeLists.txt")
        .replace("@NANO_ROS_ROOT@", workspace_root().to_str().unwrap());
    fs::write(&top, rendered).expect("write rendered CMakeLists");
    let root = dst.path().to_path_buf();
    (dst, root)
}

#[test]
fn esp_idf_esp32c3_2_component_bringup_builds() {
    // Phase 212.H.5 prereqs: nros CLI + a usable ESP-IDF installation.
    if !nros_tests::require_nros_cli() {
        nros_tests::skip!("nros CLI not found");
    }
    if !nros_tests::esp32::require_esp_idf() {
        nros_tests::skip!("ESP-IDF not reachable ($IDF_PATH + idf.py)");
    }

    let (_guard, root) = stage_fixture();
    let app_dir = root.join("esp_idf_app");
    let build_dir = app_dir.join("build");

    // `idf.py set-target` writes sdkconfig + adjusts the cmake cache.
    let set_target = Command::new("idf.py")
        .arg("-B")
        .arg(&build_dir)
        .arg("set-target")
        .arg("esp32c3")
        .current_dir(&app_dir)
        .output()
        .expect("spawn idf.py set-target");
    if !set_target.status.success() {
        // Set-target failures typically mean a half-set-up tools tree
        // (Python venv missing). Treat as a skip rather than a hard
        // fail — exercising the shim wiring is the test's contract,
        // not the maintainer's IDF install.
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
        .current_dir(&app_dir)
        .output()
        .expect("spawn idf.py build");
    assert!(
        build.status.success(),
        "idf.py build failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );

    // Sanity-check: an ELF must land at build/<project-name>.elf .
    let elf = build_dir.join("multi_pkg_workspace_esp_idf.elf");
    assert!(elf.is_file(), "missing ELF at {}", elf.display());
}
