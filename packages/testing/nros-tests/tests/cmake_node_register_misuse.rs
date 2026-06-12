//! §212.L.9 cmake-fn reject diagnostics — the configure MUST fail.
//!
//! **Runs cmake at run time — the documented exception to "No compilation
//! inside tests" (AGENTS.md / issue 0041):** a configure-*fail* with a specific
//! diagnostic can't be prebuilt as a passing fixture. The cmake configures fail
//! fast (the cmake fn raises FATAL_ERROR before any compile), so these are not
//! the timeout class; the positive metadata cases moved to build-stage fixtures
//! (`cmake_node_register_metadata.rs`).

use std::{fs, path::PathBuf, process::Command};

fn cmake_module_path() -> PathBuf {
    nros_tests::project_root().join("cmake/NanoRosNodeRegister.cmake")
}

/// Stage a fresh dir with a CMakeLists invoking the cmake fn `body`, plus dummy
/// sources. Returns (guard, root, build_dir).
fn stage(cmake_body: &str, project_name: &str) -> (tempfile::TempDir, PathBuf, PathBuf) {
    let guard = tempfile::tempdir().expect("tempdir");
    let root = guard.path().to_path_buf();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/dummy.cpp"),
        "int phase212_l9_stub() { return 0; }\n",
    )
    .unwrap();
    fs::write(
        root.join("src/dummy.c"),
        "#include <nros/node_pkg.h>\n\
         static nros_ret_t register_talker(nros_node_context_t* ctx) { (void)ctx; return NROS_RET_OK; }\n\
         NROS_NODE_REGISTER(register_talker);\n",
    )
    .unwrap();
    let cml = format!(
        "cmake_minimum_required(VERSION 3.22)\nproject({project_name} C CXX)\ninclude(\"{module}\")\n{cmake_body}\n",
        module = cmake_module_path().display(),
    );
    fs::write(root.join("CMakeLists.txt"), cml).unwrap();
    let build = root.join("build");
    (guard, root, build)
}

fn configure(root: &PathBuf, build: &PathBuf) -> std::process::Output {
    Command::new("cmake")
        .args(["-S", "."])
        .arg("-B")
        .arg(build)
        .current_dir(root)
        .output()
        .expect("spawn cmake configure")
}

#[test]
fn nano_ros_node_register_rejects_class_pkg_mismatch() {
    if !nros_tests::process::require_cmake() {
        nros_tests::skip!("cmake not on PATH");
    }
    let body = "nano_ros_node_register(\n  NAME talker\n  CLASS wrong_pkg::Talker\n  SOURCES src/dummy.cpp\n  DEPLOY native)\n";
    let (_g, root, build) = stage(body, "talker_pkg");
    let out = configure(&root, &build);
    assert!(
        !out.status.success(),
        "expected cmake configure to fail on CLASS pkg mismatch"
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("must start with 'talker_pkg::'") || err.contains("Phase 212.L.4"),
        "expected L.4 diagnostic, got:\n{err}"
    );
}

#[test]
fn nano_ros_application_rejects_embedded_deploy() {
    if !nros_tests::process::require_cmake() {
        nros_tests::skip!("cmake not on PATH");
    }
    let body =
        "nano_ros_application(\n  NAME my_app\n  SOURCES src/dummy.cpp\n  DEPLOY native zephyr)\n";
    let (_g, root, build) = stage(body, "my_app");
    let out = configure(&root, &build);
    assert!(
        !out.status.success(),
        "expected cmake configure to fail on embedded DEPLOY in Application"
    );
    let err = String::from_utf8_lossy(&out.stderr);
    // `nano_ros_application` is now a deprecated shim → `nano_ros_entry`; accept
    // the entry-layer board-centric wording or the legacy L.2 wording.
    assert!(
        err.contains("native-only")
            || err.contains("Phase 212.L.2")
            || err.contains("embedded Entry pkgs need a Board")
            || err.contains("rejected"),
        "expected an embedded-deploy rejection diagnostic, got:\n{err}"
    );
}
