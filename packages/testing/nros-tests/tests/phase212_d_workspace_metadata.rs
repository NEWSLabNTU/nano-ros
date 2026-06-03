//! Phase 212.D / 212.M.10 — cmake-fn metadata tests.
//!
//! Originally covered the pre-212 `nano_ros_workspace_metadata()` fn
//! (sidecar-TOML / `nros-plan.json` shape). §212.L retired that path
//! in favour of `nano_ros_component_register(...)` / `nano_ros_entry(...)`
//! / `nano_ros_deploy(...)`, which emit
//! `${CMAKE_BINARY_DIR}/nros-metadata.json` directly. Phase 212.M.10
//! migrated the `multi_pkg_workspace_cpp` fixture to the new shape and
//! these assertions follow suit.
//!
//! Three coverage points:
//! 1. `cmake_workspace_metadata_emits_components_cmake` — configure
//!    produces `${CMAKE_BINARY_DIR}/nros-metadata.json` containing the
//!    expected component + application + deploy entries.
//! 2. `cmake_pure_cpp_multi_component_builds` — `cmake --build` carries
//!    a 2-component pure-C++ fixture to a runnable Entry pkg binary.
//! 3. `cmake_mixed_corrosion_bridge_builds` — same shape but talker is
//!    Rust + Corrosion-bridged. `#[ignore]`d when Corrosion isn't
//!    installed (Corrosion isn't a Phase 212 prerequisite; the fixture
//!    is informational on hosts without it). Also currently
//!    `#[ignore]`d pending §212.M.10-equivalent migration of the
//!    `multi_pkg_workspace_mixed` fixture (still on the pre-212
//!    sidecar shape).
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
    workspace_root()
        .join("packages/testing/nros-tests/fixtures")
        .join(name)
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
    // Prepend ~/.nros/sdk/corrosion to CMAKE_PREFIX_PATH so the
    // workspace-installed Corrosion (per `docs/development/sdk-tiers.md`)
    // gets discovered without polluting the host system.
    let nros_corrosion = std::env::var("HOME")
        .map(|h| format!("{h}/.nros/sdk/corrosion"))
        .unwrap_or_default();
    let prefix_path = match std::env::var("CMAKE_PREFIX_PATH") {
        Ok(existing) if !existing.is_empty() => format!("{nros_corrosion}:{existing}"),
        _ => nros_corrosion,
    };
    Command::new("cmake")
        .env("CMAKE_PREFIX_PATH", &prefix_path)
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

    // §212.L.9 cmake fns emit a single metadata JSON at the build root.
    let metadata = build_dir.join("nros-metadata.json");
    assert!(
        metadata.is_file(),
        "expected {} to be emitted by the §212.L cmake fns",
        metadata.display()
    );
    let body = fs::read_to_string(&metadata).expect("read nros-metadata.json");
    assert!(
        body.contains("\"name\": \"talker\"") && body.contains("\"class\": \"talker_pkg::Talker\""),
        "metadata missing talker component entry:\n{body}"
    );
    assert!(
        body.contains("\"name\": \"listener\"")
            && body.contains("\"class\": \"listener_pkg::Listener\""),
        "metadata missing listener component entry:\n{body}"
    );
    assert!(
        body.contains("\"name\": \"demo_entry\""),
        "metadata missing demo_entry application entry:\n{body}"
    );
    assert!(
        body.contains("\"native\""),
        "metadata missing native deploy target:\n{body}"
    );
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

    // §212.L taxonomy: Node pkgs build to STATIC libs and only the
    // Entry pkg becomes an executable. The fixture's Entry pkg is
    // `demo_entry` (§212.L.3 — replaces the retired Bringup pkg).
    let demo_entry = build_dir.join("src/demo_entry/demo_entry");
    assert!(
        demo_entry.is_file(),
        "missing Entry pkg binary at {}",
        demo_entry.display()
    );
}

/// Same CMAKE_PREFIX_PATH layering as `corrosion_available()` — used by
/// the configure/build steps that actually consume Corrosion's package.
fn cmake_prefix_path_with_corrosion() -> String {
    let nros_corrosion = std::env::var("HOME")
        .map(|h| format!("{h}/.nros/sdk/corrosion"))
        .unwrap_or_default();
    match std::env::var("CMAKE_PREFIX_PATH") {
        Ok(existing) if !existing.is_empty() => format!("{nros_corrosion}:{existing}"),
        _ => nros_corrosion,
    }
}

#[test]
fn cmake_mixed_corrosion_bridge_builds() {
    if require_test_prereqs().is_none() {
        nros_tests::skip!("prereqs missing (nros CLI / cmake)");
    }
    if !corrosion_available() {
        nros_tests::skip!("Corrosion not found via cmake --find-package");
    }

    let (_guard, root) = stage_fixture("multi_pkg_workspace_mixed");
    let build_dir = root.join("build");
    let prefix_path = cmake_prefix_path_with_corrosion();

    let configure = Command::new("cmake")
        .env("CMAKE_PREFIX_PATH", &prefix_path)
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
        .env("CMAKE_PREFIX_PATH", &prefix_path)
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
