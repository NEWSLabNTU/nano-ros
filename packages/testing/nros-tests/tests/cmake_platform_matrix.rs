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
//!
//! ## The configure runs in the BUILD stage (issue 1620)
//!
//! This file used to configure at test time, as the labelled exception to "No
//! compilation inside tests": the configure must FAIL, and "a fixture whose
//! configure fails fails the BUILD". That is true only of a fixture whose
//! artifact is the configured tree. The fixture here is a VERDICT — the
//! `cmake_platform_threadx_requires_board` row (`builder =
//! "cmake-configure-verdict"`, project in `fixtures/cmake_platform_requires_board/`)
//! configures, records the exit status and stderr, and succeeds whatever cmake
//! said; the test asserts the recorded verdict. Running it here had also made
//! this file fail in a full-crate parallel run while passing alone.

use nros_tests::{TestResult, fixtures::require_compile_verdict};

// POSIX dispatch + cross-compile platforms intentionally have no smoke cell
// here — see the module header. The one remaining cell is the
// non-overlapping FATAL_ERROR check.

#[test]
fn cmake_platform_threadx_requires_board() -> TestResult<()> {
    // Phase 150.D — rewritten from the original
    // `cmake_platform_baremetal_requires_board` after Phase 138
    // collapsed "baremetal" into per-board platform values
    // (`freertos_armcm3`, `threadx_linux`, `threadx_riscv64`,
    // `threadx`+board, …). The only platform whose CMakeLists.txt
    // still requires a separate `NANO_ROS_BOARD` value today is
    // `threadx`, which disambiguates the std-vs-no_std split between
    // `threadx-linux` (host libc) and `rv-virt-threadx` (bare-metal).
    //
    // Verifies: `NANO_ROS_PLATFORM=threadx` without `NANO_ROS_BOARD`
    // FATAL_ERRORs at configure time and the error message mentions
    // NANO_ROS_BOARD.
    let v = require_compile_verdict("cmake_platform_threadx_requires_board")?.outcome;
    assert!(
        !v.success(),
        "expected NANO_ROS_PLATFORM=threadx without NANO_ROS_BOARD to FATAL_ERROR at \
         configure time, but cmake exited 0.\nstdout:\n{}\nstderr:\n{}",
        v.stdout,
        v.stderr
    );
    assert!(
        v.stderr.contains("NANO_ROS_BOARD"),
        "expected the FATAL_ERROR message to mention NANO_ROS_BOARD; got:\n{}",
        v.stderr
    );
    Ok(())
}
