//! Phase 137.4 — in-tree consumer smoke test.
//!
//! Verifies that a minimal user project consuming nano-ros via
//! `add_subdirectory(<repo-root>)` configures, builds, and links
//! against `NanoRos::NanoRos`. No `just install-local` step is run —
//! this is the canonical proof-of-concept for the source-distribution
//! direction (Phase 137 / 138 / 139 / 140).
//!
//! Coverage:
//! - POSIX platform, zenoh RMW (the only branch fully wired in 137.1).
//! - Calls `nros_support_get_zero_initialized()` from `main.c` so the
//!   linker has to resolve at least one nros symbol — link-correctness
//!   is what matters, not runtime behaviour.
//!
//! Failure modes the test catches:
//! - Missing umbrella `NanoRos::NanoRos` target.
//! - Variant header `nros_config_generated.h` not on the include path
//!   (Phase 137 in-tree mirror regression).
//! - Static-archive link-order regressions (RMW staticlib + platform
//!   shim ordering inside the umbrella target's INTERFACE_LINK_LIBRARIES).
//!
//! Test FAILS (not skips) when cmake / a C compiler are absent — Phase 137
//! presumes a working host toolchain, matching the project-wide "no
//! silent skip" rule from CLAUDE.md.
//!
//! Skip-via-panic (`nros_tests::skip!`) only when the codegen submodule
//! at `packages/codegen/` isn't initialised — in that case the in-tree
//! build cannot complete and the failure has nothing to do with Phase 137.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR for nros-tests is .../packages/testing/nros-tests.
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .ancestors()
        .nth(3)
        .expect("workspace root above CARGO_MANIFEST_DIR")
        .to_path_buf()
}

const USER_CMAKE: &str = r#"cmake_minimum_required(VERSION 3.22)
project(smoke C)

set(NANO_ROS_PLATFORM posix)
set(NANO_ROS_RMW     zenoh)

add_subdirectory("@NANO_ROS_ROOT@" nano_ros)

add_executable(smoke main.c)
target_link_libraries(smoke PRIVATE NanoRos::NanoRos)
"#;

const USER_MAIN_C: &str = r#"/* Phase 137.4 smoke test — link-correctness only. */
#include <nros/init.h>

int main(void) {
    nros_support_t support = nros_support_get_zero_initialized();
    (void)support;
    return 0;
}
"#;

#[test]
fn cmake_add_subdirectory_smoke() {
    let root = workspace_root();
    // Phase 137.2 depends on the codegen submodule being checked out;
    // without it the in-tree include() can't reach
    // `NanoRosGenerateInterfaces.cmake` and the root CMakeLists never
    // hits a steady state. Skip cleanly so a fresh worktree without
    // submodules surfaces the right signal.
    let codegen_marker = root.join(
        "packages/codegen/packages/nros-codegen-c/cmake/NanoRosGenerateInterfaces.cmake",
    );
    if !codegen_marker.exists() {
        nros_tests::skip!(
            "codegen submodule not initialised — run `git submodule update --init packages/codegen` first"
        );
    }

    // Use a stable workspace under tmp/ so build artifacts can be
    // inspected after a failure (matches the project-wide "Temp files
    // in $project/tmp/" rule from CLAUDE.md).
    let tmp = root.join("tmp").join("phase-137-smoke");
    if tmp.exists() {
        fs::remove_dir_all(&tmp).expect("clear previous tmp dir");
    }
    let user = tmp.join("user_project");
    let build = tmp.join("build");
    fs::create_dir_all(&user).expect("create user_project dir");

    let cmake_body = USER_CMAKE.replace("@NANO_ROS_ROOT@", root.to_str().unwrap());
    fs::write(user.join("CMakeLists.txt"), cmake_body).expect("write user CMakeLists.txt");
    fs::write(user.join("main.c"), USER_MAIN_C).expect("write user main.c");

    // Configure.
    let configure = Command::new("cmake")
        .args([
            "-S",
            user.to_str().unwrap(),
            "-B",
            build.to_str().unwrap(),
        ])
        .output()
        .expect("failed to invoke cmake configure");
    assert!(
        configure.status.success(),
        "cmake configure failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&configure.stdout),
        String::from_utf8_lossy(&configure.stderr)
    );

    // Build.
    let build_cmd = Command::new("cmake")
        .args(["--build", build.to_str().unwrap(), "--target", "smoke"])
        .output()
        .expect("failed to invoke cmake --build");
    assert!(
        build_cmd.status.success(),
        "cmake --build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build_cmd.stdout),
        String::from_utf8_lossy(&build_cmd.stderr)
    );

    // Assert the smoke binary exists. Link-correctness — no runtime call.
    let smoke = build.join("smoke");
    assert!(
        smoke.exists(),
        "smoke binary not produced at {} — Phase 137 entry point regression",
        smoke.display()
    );
}
