//! Phase 212.D — `nano_ros_workspace_metadata()` cmake function tests.
//!
//! Three coverage points:
//! 1. `cmake_workspace_metadata_emits_components_cmake` — configure-time
//!    invocation produces `${CMAKE_BINARY_DIR}/nros_components.cmake`
//!    containing per-component marker targets parsed from `nros-plan.json`.
//! 2. `cmake_pure_cpp_multi_component_builds` — `cmake --build` carries
//!    a 2-component pure-C++ fixture to runnable binaries.
//! 3. `cmake_mixed_corrosion_bridge_builds` — same shape but talker is
//!    Rust + Corrosion-bridged. `#[ignore]`d when Corrosion isn't
//!    installed (Corrosion isn't a Phase 212 prerequisite; the fixture
//!    is informational on hosts without it).
//!
//! All three skip cleanly via `nros_tests::skip!` if the `nros` CLI or
//! `cmake` aren't available — mirrors `cmake_add_subdirectory_smoke`'s
//! pattern.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn workspace_root() -> PathBuf {
    nros_tests::project_root()
}

fn fixture(name: &str) -> PathBuf {
    workspace_root().join("packages/testing/nros-tests/fixtures").join(name)
}

/// Stage a fixture into a tempdir with `@NANO_ROS_ROOT@` rewritten so
/// the test doesn't write into the source tree.
fn stage_fixture(name: &str) -> (tempfile::TempDir, PathBuf) {
    let src = fixture(name);
    let dst = tempfile::tempdir().expect("tempdir");
    copy_tree(&src, dst.path()).expect("copy fixture");
    // Rewrite NANO_ROS_ROOT placeholder in the top-level CMakeLists.txt.
    let top = dst.path().join("CMakeLists.txt");
    let rendered = fs::read_to_string(&top)
        .expect("read top CMakeLists")
        .replace("@NANO_ROS_ROOT@", workspace_root().to_str().unwrap());
    fs::write(&top, rendered).expect("write rendered CMakeLists");
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

/// Check if Corrosion is discoverable by cmake (system pkg). Returns
/// false when not — gate for the mixed test.
fn corrosion_available() -> bool {
    // `cmake --find-package` writes CMakeFiles/ into the cwd; route it
    // through a tempdir so we don't pollute the workspace root.
    let probe = match tempfile::tempdir() {
        Ok(d) => d,
        Err(_) => return false,
    };
    Command::new("cmake")
        .args([
            "--find-package",
            "-DNAME=Corrosion",
            "-DCOMPILER_ID=GNU",
            "-DLANGUAGE=CXX",
            "-DMODE=EXIST",
        ])
        .current_dir(probe.path())
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn require_test_prereqs() -> Option<()> {
    if !nros_tests::require_nros_cli() {
        return None;
    }
    if !nros_tests::process::require_cmake() {
        return None;
    }
    Some(())
}

#[test]
fn cmake_workspace_metadata_emits_components_cmake() {
    if require_test_prereqs().is_none() {
        nros_tests::skip!("prereqs missing (nros CLI / cmake)");
    }

    let (_guard, root) = stage_fixture("multi_pkg_workspace_cpp");
    let build_dir = root.join("build");

    let out = Command::new("cmake")
        .args(["-S", "."])
        .arg("-B")
        .arg(&build_dir)
        .current_dir(&root)
        .output()
        .expect("spawn cmake configure");
    assert!(
        out.status.success(),
        "cmake configure failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let components_cmake = build_dir.join("nros_components.cmake");
    assert!(
        components_cmake.is_file(),
        "expected {} to be written by nano_ros_workspace_metadata()",
        components_cmake.display()
    );
    let body = fs::read_to_string(&components_cmake).expect("read components.cmake");
    assert!(
        body.contains("nros_component_talker_pkg_talker")
            && body.contains("nros_component_listener_pkg_listener"),
        "components.cmake missing expected component targets:\n{body}"
    );
    assert!(
        body.contains("NROS_LANGUAGE \"cpp\""),
        "components.cmake missing language property:\n{body}"
    );

    // `nros plan` artifact survives at the documented location.
    let plan_json = build_dir.join("nros-plan/nros-plan.json");
    assert!(plan_json.is_file(), "missing {}", plan_json.display());
}

#[test]
fn cmake_pure_cpp_multi_component_builds() {
    if require_test_prereqs().is_none() {
        nros_tests::skip!("prereqs missing (nros CLI / cmake)");
    }

    let (_guard, root) = stage_fixture("multi_pkg_workspace_cpp");
    let build_dir = root.join("build");

    let configure = Command::new("cmake")
        .args(["-S", "."])
        .arg("-B")
        .arg(&build_dir)
        .current_dir(&root)
        .output()
        .expect("spawn cmake configure");
    assert!(
        configure.status.success(),
        "cmake configure failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&configure.stdout),
        String::from_utf8_lossy(&configure.stderr)
    );

    let build = Command::new("cmake")
        .arg("--build")
        .arg(&build_dir)
        .output()
        .expect("spawn cmake build");
    assert!(
        build.status.success(),
        "cmake build failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );

    let talker = build_dir.join("src/talker_pkg/talker");
    let listener = build_dir.join("src/listener_pkg/listener");
    assert!(talker.is_file(), "missing talker binary at {}", talker.display());
    assert!(listener.is_file(), "missing listener binary at {}", listener.display());
}

#[test]
#[ignore = "requires Corrosion on the host; un-ignore once added to setup tier"]
fn cmake_mixed_corrosion_bridge_builds() {
    if require_test_prereqs().is_none() {
        nros_tests::skip!("prereqs missing (nros CLI / cmake)");
    }
    if !corrosion_available() {
        nros_tests::skip!("Corrosion not found via cmake --find-package");
    }

    let (_guard, root) = stage_fixture("multi_pkg_workspace_mixed");
    let build_dir = root.join("build");

    let configure = Command::new("cmake")
        .args(["-S", "."])
        .arg("-B")
        .arg(&build_dir)
        .current_dir(&root)
        .output()
        .expect("spawn cmake configure");
    assert!(
        configure.status.success(),
        "cmake configure failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&configure.stdout),
        String::from_utf8_lossy(&configure.stderr)
    );

    let build = Command::new("cmake")
        .arg("--build")
        .arg(&build_dir)
        .output()
        .expect("spawn cmake build");
    assert!(
        build.status.success(),
        "cmake build failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );

    let listener = build_dir.join("src/listener_pkg/listener");
    assert!(
        listener.is_file(),
        "missing cpp listener binary at {}",
        listener.display()
    );
}
