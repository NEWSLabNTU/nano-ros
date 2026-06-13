//! §212.H.4 — ThreadX adapter codegen + Corrosion-bridge audit —
//! build-stage fixture (issue 0041 Wave B).
//!
//! Verifies the codegen + Corrosion-bridge surface of the ThreadX
//! platform module (`cmake/platform/nano-ros-threadx.cmake` →
//! `cmake/NanoRosThreadxSystemCodegen.cmake`):
//!
//! 1. `nros_threadx_codegen_system(SYSTEM <bringup>)` shells `nros plan`
//!    at cmake configure time, emits `nros-system/system_main.c` with one
//!    extern + weak-stub + dispatch entry per planned component, and
//!    compiles it into the `nros_system_main` STATIC target.
//! 2. The fixture's `threadx_app` executable links `nros_system_main` via
//!    `nros_threadx_link_app(<target>)`, runs, and the talker + listener
//!    component entries fire in plan order. With Corrosion provisioned
//!    (`nros setup --tool corrosion`) the codegen helper imports the Rust
//!    component crates, so the REAL (non-stub) entries land in the binary.
//!
//! ## issue-0041 conversion
//!
//! Per "No compilation inside tests", the cmake configure + build + the
//! Corrosion-imported Rust crate compiles run in the **build stage**:
//!
//! * `threadx_bringup` — `build_cmake_fixture` configures + builds the host
//!   `threadx_app` (Corrosion on `CMAKE_PREFIX_PATH` from the
//!   `nros setup`-provisioned `~/.nros/sdk/corrosion`). This test INSPECTS
//!   the prebuilt codegen artifacts + RUNS the prebuilt binary — no cmake at
//!   run time. The build needs `corrosion` + cmake + `nros` + the
//!   `play_launch_parser` (gated by `cmake_fixture_prereqs_ok`); absent →
//!   no artifact → `require_cmake_fixture` skips per tier.
//! * `threadx_bringup_rv64` — the RISC-V 64 QEMU codegen sibling
//!   (CONFIGURE-ONLY: `main.c` is host-shaped, won't link bare-metal rv64).
//!   Proves the codegen is board-agnostic under `-DNANO_ROS_BOARD=riscv64-qemu`.
//!   Built only when `riscv64-unknown-elf-gcc` + the `nros setup --source
//!   threadx --source threadx-netxduo` trees are present; else skipped.
//!
//! A full ThreadX-Linux native-simulation bringup (kernel boot, NetX BSD
//! shim, zenohd over veth, real publish/subscribe) is exercised by
//! `tests/rtos_e2e.rs` Platform::ThreadxLinux — outside this audit.
//
// NOTE: the C-side `system_main.c` baker still emits
// `__nros_component_<pkg>_register` extern decls + weak stubs, which the
// linker resolves against the Corrosion-imported Rust staticlibs that
// re-expose those symbols via per-fixture trampolines (see
// `multi_pkg_workspace_threadx/src/<pkg>/src/lib.rs`). Re-audit when the
// ThreadX Entry-pkg migration replaces the C baker.

use std::{fs, path::PathBuf, process::Command};

fn cmake_fixture_dir(id: &str) -> PathBuf {
    nros_tests::project_root().join("build/cmake-fixtures").join(id)
}

/// Assert the codegen artifacts a `nros_threadx_codegen_system(...)` configure
/// emits under `<build>/` — shared by the host + rv64 legs (board-agnostic).
fn assert_codegen_artifacts(build_dir: &std::path::Path) {
    let sys_main = build_dir.join("nros-system/system_main.c");
    let sys_cargo = build_dir.join("nros-system/Cargo.toml");
    let components_cmake = build_dir.join("nros_components.cmake");
    assert!(sys_main.is_file(), "missing {}", sys_main.display());
    assert!(sys_cargo.is_file(), "missing {}", sys_cargo.display());
    assert!(
        components_cmake.is_file(),
        "missing {}",
        components_cmake.display()
    );

    let sys_main_body = fs::read_to_string(&sys_main).expect("read system_main.c");
    assert!(
        sys_main_body.contains("__nros_component_talker_pkg_register")
            && sys_main_body.contains("__nros_component_listener_pkg_register"),
        "system_main.c missing per-component register entries:\n{sys_main_body}"
    );

    let cargo_stub = fs::read_to_string(&sys_cargo).expect("read Cargo.toml");
    assert!(
        cargo_stub.contains("src/talker_pkg") && cargo_stub.contains("src/listener_pkg"),
        "workspace Cargo.toml stub missing component members:\n{cargo_stub}"
    );
}

/// §212.H.4 main acceptance: the prebuilt host `threadx_app` carries the
/// codegen artifacts AND, when run, dispatches the talker + listener
/// component entries in plan order.
#[test]
fn threadx_linux_2_component_bringup_builds_and_publishes() -> nros_tests::TestResult<()> {
    // Build-stage `build_cmake_fixture` produced the host binary; resolve it
    // (tier-aware skip when the cmake/corrosion fixture wasn't built).
    let app = nros_tests::fixtures::require_cmake_fixture("threadx_bringup", "threadx_app")?;
    let build_dir = cmake_fixture_dir("threadx_bringup");

    assert_codegen_artifacts(&build_dir);

    // Run the prebuilt binary + assert the per-component dispatch fires.
    let run = Command::new(&app).output().expect("spawn threadx_app");
    assert!(
        run.status.success(),
        "threadx_app exited non-zero:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    let out = String::from_utf8_lossy(&run.stdout);
    eprintln!("--- threadx_app stdout ---\n{out}--- end ---");
    assert!(
        out.contains("[nros-system] spawning components"),
        "no system_main banner:\n{out}"
    );
    assert!(out.contains("[talker]"), "no talker dispatch:\n{out}");
    assert!(out.contains("[listener]"), "no listener dispatch:\n{out}");
    Ok(())
}

/// Corrosion-present variant — the build stage put the `nros setup`-provisioned
/// Corrosion on `CMAKE_PREFIX_PATH`, so the codegen helper imported the Rust
/// component crates and the REAL (non-stub) entries are in the binary. Asserts
/// the prebuilt run shows the real-entry messages (the weak-stub fallback prints
/// `stub component entry` instead). When the build stage lacked Corrosion the
/// binary still runs (weak stubs) → the real-entry assertion would fail, so we
/// skip that case rather than misreport.
#[test]
fn threadx_linux_2_component_bringup_corrosion_imports_rust() -> nros_tests::TestResult<()> {
    let app = nros_tests::fixtures::require_cmake_fixture("threadx_bringup", "threadx_app")?;

    let run = Command::new(&app).output().expect("spawn threadx_app");
    let out = String::from_utf8_lossy(&run.stdout);

    // Weak-stub build (Corrosion absent at build time) → no real import to
    // assert. Skip with the reason instead of failing.
    if out.contains("stub component entry") {
        nros_tests::skip!(
            "threadx_bringup fixture built without Corrosion (weak stubs) — no Rust import to \
             assert. Provision via `nros setup --tool corrosion` before `just build-test-fixtures`."
        );
    }
    assert!(
        out.contains("[talker] component entry reached")
            && out.contains("[listener] component entry reached"),
        "Corrosion-imported Rust entries didn't fire:\n{out}"
    );
    Ok(())
}

/// §212.H.4 sibling — RISC-V 64 QEMU codegen + platform-module dispatch
/// verification. Configure-only (the fixture's host-shaped `main.c` won't link
/// bare-metal rv64); the build stage emits the SAME codegen artifacts under
/// `-DNANO_ROS_BOARD=riscv64-qemu`, proving the codegen path is board-agnostic.
/// The `threadx_bringup_rv64` fixture is built only when
/// `riscv64-unknown-elf-gcc` + the ThreadX/NetX trees are present (`nros setup
/// --source threadx --source threadx-netxduo`); absent → skip.
#[test]
fn threadx_riscv64_qemu_2_component_bringup_builds() -> nros_tests::TestResult<()> {
    // Resolve a configure artifact (no binary — configure-only). Tier-aware
    // skip when the rv64 fixture wasn't built (riscv toolchain / trees absent).
    let sys_main =
        nros_tests::fixtures::require_cmake_fixture("threadx_bringup_rv64", "nros-system/system_main.c")?;
    let build_dir = sys_main
        .parent()
        .and_then(|p| p.parent())
        .expect("rv64 build dir");

    // Same codegen surface as the host leg — board-agnostic emit.
    assert_codegen_artifacts(build_dir);
    Ok(())
}
