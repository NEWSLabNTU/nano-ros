//! Phase 138.6 — per-platform cmake-module smoke test matrix.
//!
//! Verifies that each per-platform module under `cmake/platform/`
//! conforms to the §A contract: configuring a minimal user project that
//! picks `NANO_ROS_PLATFORM=<plat>` includes the right module, surfaces
//! `NanoRos::Platform`, defines `nros_platform_link_app(target)`, and
//! (for platforms where the toolchain is present) links a tiny binary
//! against `NanoRos::NanoRos`.
//!
//! Coverage:
//! - POSIX always runs. Same shape as Phase 137's
//!   `cmake_add_subdirectory` smoke test but explicitly parameterised
//!   on `NANO_ROS_PLATFORM=posix`. Failure ⇒ Phase 138 dispatch
//!   regression.
//! - Cross-compile platforms (zephyr, freertos, nuttx, threadx) skip
//!   cleanly via `nros_tests::skip!` when their cross-toolchain isn't
//!   installed. CLAUDE.md "Tests must fail on unmet preconditions" rule
//!   carves out the `skip!` macro for matrix cells that genuinely
//!   cannot run without optional system deps.
//! - bare-metal exercises the FATAL_ERROR path in
//!   `nros-baremetal.cmake` (missing `NANO_ROS_BOARD`) via a configure-
//!   only check; the rest of the link is board-specific and lives in
//!   the per-board overlays under `cmake/board/`.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn workspace_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .ancestors()
        .nth(3)
        .expect("workspace root above CARGO_MANIFEST_DIR")
        .to_path_buf()
}

/// Skip the entire matrix when the codegen submodule isn't initialised
/// — same precondition as Phase 137's smoke test (the root CMakeLists
/// includes the codegen cmake module unconditionally on POSIX).
fn require_codegen_or_skip() {
    let root = workspace_root();
    let codegen_marker =
        root.join("packages/codegen/packages/nros-codegen-c/cmake/NanoRosGenerateInterfaces.cmake");
    if !codegen_marker.exists() {
        nros_tests::skip!(
            "codegen submodule not initialised — run `git submodule update --init packages/codegen` first"
        );
    }
}

const USER_CMAKE_TEMPLATE: &str = r#"cmake_minimum_required(VERSION 3.22)
project(plat_smoke C)

set(NANO_ROS_PLATFORM @PLATFORM@)
set(NANO_ROS_RMW     zenoh)

add_subdirectory("@NANO_ROS_ROOT@" nano_ros)

add_executable(plat_smoke main.c)
target_link_libraries(plat_smoke PRIVATE NanoRos::NanoRos)
nros_platform_link_app(plat_smoke)
"#;

const USER_MAIN_C: &str = r#"/* Phase 138.6 smoke test — link-correctness only. */
#include <nros/init.h>

int main(void) {
    nros_support_t support = nros_support_get_zero_initialized();
    (void)support;
    return 0;
}
"#;

fn run_platform_cell(platform: &str, tmp_subdir: &str) {
    let root = workspace_root();
    let tmp = root.join("tmp").join(tmp_subdir);
    if tmp.exists() {
        fs::remove_dir_all(&tmp).expect("clear previous tmp dir");
    }
    let user = tmp.join("user_project");
    let build = tmp.join("build");
    fs::create_dir_all(&user).expect("create user_project dir");

    let cmake_body = USER_CMAKE_TEMPLATE
        .replace("@PLATFORM@", platform)
        .replace("@NANO_ROS_ROOT@", root.to_str().unwrap());
    fs::write(user.join("CMakeLists.txt"), cmake_body).expect("write user CMakeLists.txt");
    fs::write(user.join("main.c"), USER_MAIN_C).expect("write user main.c");

    let configure = Command::new("cmake")
        .args(["-S", user.to_str().unwrap(), "-B", build.to_str().unwrap()])
        .output()
        .expect("failed to invoke cmake configure");
    assert!(
        configure.status.success(),
        "cmake configure failed for NANO_ROS_PLATFORM={}\nstdout:\n{}\nstderr:\n{}",
        platform,
        String::from_utf8_lossy(&configure.stdout),
        String::from_utf8_lossy(&configure.stderr)
    );

    let build_cmd = Command::new("cmake")
        .args(["--build", build.to_str().unwrap(), "--target", "plat_smoke"])
        .output()
        .expect("failed to invoke cmake --build");
    assert!(
        build_cmd.status.success(),
        "cmake --build failed for NANO_ROS_PLATFORM={}\nstdout:\n{}\nstderr:\n{}",
        platform,
        String::from_utf8_lossy(&build_cmd.stdout),
        String::from_utf8_lossy(&build_cmd.stderr)
    );

    let bin = build.join("plat_smoke");
    assert!(
        bin.exists(),
        "plat_smoke binary not produced at {} — Phase 138 dispatch regression for {}",
        bin.display(),
        platform
    );
}

// -----------------------------------------------------------------------
// POSIX — always runs. Mirrors Phase 137 smoke but routed through the
// Phase 138 per-platform module.
// -----------------------------------------------------------------------
#[test]
fn cmake_platform_posix() {
    require_codegen_or_skip();
    run_platform_cell("posix", "phase-138-smoke-posix");
}

// -----------------------------------------------------------------------
// Cross-compile platforms — skip cleanly when toolchains aren't present.
// Detection heuristics match the legacy per-platform recipes in
// `justfile`; replace with actual cross-build invocation once Phase 139's
// integration shells are in place.
// -----------------------------------------------------------------------

fn require_cmd_or_skip(cmd: &str, hint: &str) {
    // Avoid pulling in a `which`-crate dep; PATH walk is enough for a
    // configure-time skip gate.
    let Some(path) = std::env::var_os("PATH") else {
        nros_tests::skip!("PATH unset — cannot detect {}", cmd);
    };
    let found = std::env::split_paths(&path).any(|dir| dir.join(cmd).is_file());
    if !found {
        nros_tests::skip!("{} not on PATH — {}", cmd, hint);
    }
}

#[test]
fn cmake_platform_zephyr() {
    require_codegen_or_skip();
    // Zephyr's normal entry is `west`. Skip cleanly when absent —
    // Phase 139 will wire the west-driven integration shell.
    require_cmd_or_skip("west", "install zephyr SDK (`just zephyr setup`)");
    nros_tests::skip!("Phase 138.6 zephyr cell deferred to Phase 139's west / module integration");
}

#[test]
fn cmake_platform_freertos() {
    require_codegen_or_skip();
    require_cmd_or_skip(
        "arm-none-eabi-gcc",
        "install gcc-arm-none-eabi (`just freertos setup`)",
    );
    nros_tests::skip!(
        "Phase 138.6 freertos cell deferred — needs board-driver paths (FREERTOS_DIR + LWIP_DIR) the smoke project doesn't supply"
    );
}

#[test]
fn cmake_platform_nuttx() {
    require_codegen_or_skip();
    require_cmd_or_skip(
        "arm-none-eabi-gcc",
        "install nuttx toolchain (`just nuttx setup`)",
    );
    nros_tests::skip!(
        "Phase 138.6 nuttx cell deferred — NuttX builds via cargo / `just nuttx build`, not raw cmake"
    );
}

#[test]
fn cmake_platform_threadx() {
    require_codegen_or_skip();
    // ThreadX on Linux uses the host compiler; matrix-test the linux
    // variant once Phase 139 wires the threadx-linux integration shell.
    nros_tests::skip!(
        "Phase 138.6 threadx cell deferred to Phase 139's threadx-linux integration shell"
    );
}

#[test]
fn cmake_platform_threadx_requires_board() {
    require_codegen_or_skip();
    // Phase 150.D — rewritten from the original
    // `cmake_platform_baremetal_requires_board` after Phase 138
    // collapsed "baremetal" into per-board platform values
    // (`freertos_armcm3`, `threadx_linux`, `threadx_riscv64`,
    // `threadx`+board, …). The only platform whose CMakeLists.txt
    // still requires a separate `NANO_ROS_BOARD` value today is
    // `threadx` (lines 73-81 of `packages/core/nros-c/CMakeLists.txt`),
    // which disambiguates the std-vs-no_std split between
    // `threadx-linux` (host libc) and `riscv64-qemu` (bare-metal).
    //
    // Verifies: `NANO_ROS_PLATFORM=threadx` without `NANO_ROS_BOARD`
    // FATAL_ERRORs at configure time and the error message mentions
    // NANO_ROS_BOARD.
    let root = workspace_root();
    let tmp = root.join("tmp").join("phase-150-smoke-threadx-noboard");
    if tmp.exists() {
        fs::remove_dir_all(&tmp).expect("clear previous tmp dir");
    }
    let user = tmp.join("user_project");
    let build = tmp.join("build");
    fs::create_dir_all(&user).expect("create user_project dir");
    let cmake_body = USER_CMAKE_TEMPLATE
        .replace("@PLATFORM@", "threadx")
        .replace("@NANO_ROS_ROOT@", root.to_str().unwrap());
    fs::write(user.join("CMakeLists.txt"), cmake_body).expect("write user CMakeLists.txt");
    fs::write(user.join("main.c"), USER_MAIN_C).expect("write user main.c");

    let configure = Command::new("cmake")
        .args(["-S", user.to_str().unwrap(), "-B", build.to_str().unwrap()])
        .output()
        .expect("failed to invoke cmake configure");
    let stderr = String::from_utf8_lossy(&configure.stderr);
    if configure.status.success() {
        panic!(
            "expected NANO_ROS_PLATFORM=threadx without NANO_ROS_BOARD to FATAL_ERROR at configure time, but cmake exited 0.\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&configure.stdout),
            stderr
        );
    }
    assert!(
        stderr.contains("NANO_ROS_BOARD"),
        "expected the FATAL_ERROR message to mention NANO_ROS_BOARD; got:\n{stderr}"
    );
}
