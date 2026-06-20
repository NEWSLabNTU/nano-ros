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
//! - POSIX dispatch (configure + build + `nros_platform_link_app`) lives in
//!   `cmake_add_subdirectory::cmake_add_subdirectory_smoke` — Phase 182.2
//!   merged the near-identical `cmake_platform_posix` cell into it (same
//!   clean configure+build of the same stack). This file keeps only the
//!   non-overlapping bare-metal FATAL_ERROR check below.
//! - Cross-compile platforms (zephyr, freertos, nuttx, threadx) are NOT
//!   smoke-tested here. Their `cmake/platform/<plat>.cmake` modules are
//!   exercised end-to-end by the real C/C++ example builds + `rtos_e2e`
//!   (each example configures `add_subdirectory(<root>) +
//!   NANO_ROS_PLATFORM=<plat>` with the board paths a minimal smoke can't
//!   supply) and the Phase 139 `integrations/<rtos>/` shells. The
//!   placeholder matrix cells (which only ever `skip!`ed, deferred to a
//!   never-tracked "Phase 139") were removed. POSIX guards the dispatch path.
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

/// Phase 195.D — skip the matrix when the host `nros` build tool isn't
/// installed. Phase 218 brought the CLI in-tree (`packages/cli/`, built
/// by `just setup-cli`); the root CMakeLists resolves it from `$NROS_CLI`
/// / PATH (incl `packages/cli/target/release/` via `activate.sh`) /
/// `~/.nros/bin` (transitional).
fn require_codegen_or_skip() {
    if let Some(p) = std::env::var_os("NROS_CLI")
        && Path::new(&p).is_file()
    {
        return;
    }
    if Command::new("nros")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
    {
        return;
    }
    let home = std::env::var_os("NROS_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".nros")));
    if home.map(|h| h.join("bin/nros").is_file()).unwrap_or(false) {
        return;
    }
    nros_tests::skip!(
        "nros build tool not installed — run `just setup-cli` + `source ./activate.sh` first"
    );
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

// -----------------------------------------------------------------------
// POSIX dispatch + cross-compile platforms intentionally have no smoke cell
// here:
// - The POSIX §A path (configure + build + `nros_platform_link_app`) is
//   covered by `cmake_add_subdirectory::cmake_add_subdirectory_smoke`, which
//   was a near-identical clean configure+build of the same stack; Phase 182.2
//   merged the two and kept the add_subdirectory variant (it carries the same
//   `nros_platform_link_app(target)` assertion now).
// - Cross-compile platforms (zephyr, freertos, nuttx, threadx) are covered by
//   the real C/C++ example builds + `rtos_e2e` + the Phase 139
//   `integrations/<rtos>/` shells (see the module header). Their placeholder
//   cells only ever `skip!`ed and were removed.
// The one remaining cell below is the non-overlapping FATAL_ERROR check.
// -----------------------------------------------------------------------

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
