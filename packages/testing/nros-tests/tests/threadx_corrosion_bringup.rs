//! Phase 212.H.4 — ThreadX adapter audit + alignment.
//!
//! Verifies the codegen + Corrosion-bridge surface added to the ThreadX
//! platform module (`cmake/platform/nano-ros-threadx.cmake` →
//! `cmake/NanoRosThreadxSystemCodegen.cmake`):
//!
//! 1. `nros_threadx_codegen_system(SYSTEM <bringup>)` shells `nros plan`
//!    at cmake configure time, emits
//!    `${CMAKE_BINARY_DIR}/nros-system/system_main.c` with one extern
//!    + weak-stub + dispatch entry per planned component, and compiles
//!    it into the `nros_system_main` STATIC target.
//! 2. The fixture's `threadx_app` executable links against
//!    `nros_system_main` via the `nros_threadx_link_app(<target>)`
//!    helper, runs, and the talker + listener stub component entries
//!    fire in plan order.
//!
//! The test deliberately stays scoped to the codegen + link contract.
//! A full ThreadX-Linux native-simulation bringup (kernel boot, NetX BSD
//! shim, zenohd over veth, real publish/subscribe) is exercised by
//! `tests/rtos_e2e.rs` Platform::ThreadxLinux — outside this audit.
//!
//! Skip semantics mirror `phase212_d_workspace_metadata.rs`: `nros_tests::skip!`
//! when prereqs (`nros` CLI, `cmake`) are missing.
//!
//! The Corrosion-import path is exercised lazily — when Corrosion isn't
//! installed on the host, the helper logs a STATUS message and emits a
//! weak stub for each `__nros_component_<pkg>_register()` (Phase
//! 212.M.5.a.1 per-pkg mangled symbol) so the build still links + runs.
//! A separate `#[ignore]` test asserts the Corrosion-present path
//! imports the Rust component crates.
//
// TODO(N.7 ThreadX migration): Phase 212.N.7 step-6 retired the
// Rust-side `__nros_component_<pkg>_*` extern symbols (the
// `nros::node!()` macro now emits one public `register(runtime)`
// wrapper instead). The ThreadX C-side `system_main.c` baker
// (`cmake/NanoRosThreadxSystemCodegen.cmake`) is untouched by that
// retirement — it still emits `__nros_component_<pkg>_register` extern
// declarations + weak stubs at the C layer, where the linker resolves
// them against Corrosion-imported Rust staticlibs that re-expose those
// symbols via per-fixture trampolines (see
// `multi_pkg_workspace_threadx/src/<pkg>/src/lib.rs`). The assertions
// below still match that C-side output verbatim. Re-audit when the
// ThreadX Entry pkg migration lands and the C baker is replaced (or
// rewired to call `<pkg>::register(runtime)` through Corrosion).

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

fn stage_fixture(name: &str) -> (tempfile::TempDir, PathBuf) {
    let src = fixture(name);
    let dst = tempfile::tempdir().expect("tempdir");
    copy_tree(&src, dst.path()).expect("copy fixture");
    // Rewrite @NANO_ROS_ROOT@ in threadx_app/CMakeLists.txt.
    let cml = dst.path().join("threadx_app/CMakeLists.txt");
    let rendered = fs::read_to_string(&cml)
        .expect("read threadx_app CMakeLists")
        .replace("@NANO_ROS_ROOT@", workspace_root().to_str().unwrap());
    fs::write(&cml, rendered).expect("write rendered CMakeLists");
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

fn corrosion_available() -> bool {
    let probe = match tempfile::tempdir() {
        Ok(d) => d,
        Err(_) => return false,
    };
    // Prepend ~/.nros/sdk/corrosion to CMAKE_PREFIX_PATH so the
    // user-installed Corrosion (via `just workspace install-corrosion`
    // or the manual `cmake --install` recipe) gets discovered.
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
            "-DLANGUAGE=C",
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

/// Phase 212.H.4 main acceptance: cmake codegen + link + runtime
/// component dispatch on threadx-linux (build host is also the
/// simulation host — no cross-compile dependency).
///
/// Phase 212.M-F.17 (landed): the M.10 sidecar-retirement put the
/// Cargo-native `[package.metadata.nros.component]` shape on this
/// fixture, and `nros plan` now consumes it directly via
/// `Workspace::synthetic_metadata_artifacts` (no sidecar `metadata/*.json`
/// needed). The cmake configure step that shells `nros plan` therefore
/// succeeds. Requires the in-tree CLI (`just setup-cli`, resolved on
/// PATH by `activate.sh`) carrying the M-F.17 planner wiring.
#[test]
fn threadx_linux_2_component_bringup_builds_and_publishes() {
    if require_test_prereqs().is_none() {
        nros_tests::skip!("prereqs missing (nros CLI / cmake)");
    }

    let (_guard, root) = stage_fixture("multi_pkg_workspace_threadx");
    let app_src = root.join("threadx_app");
    let build_dir = app_src.join("build");

    let configure = Command::new("cmake")
        .args(["-S"])
        .arg(&app_src)
        .args(["-B"])
        .arg(&build_dir)
        .output()
        .expect("spawn cmake configure");
    assert!(
        configure.status.success(),
        "cmake configure failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&configure.stdout),
        String::from_utf8_lossy(&configure.stderr)
    );

    // Codegen artifacts surfaced where the helper documents.
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

    let app = build_dir.join("threadx_app");
    assert!(
        app.is_file(),
        "missing threadx_app binary at {}",
        app.display()
    );

    // Run and assert the per-component dispatch fires. With Corrosion
    // present the real Rust entries print `[talker] component entry
    // reached` / `[listener] component entry reached`; without Corrosion
    // the weak stubs print `[<name>] stub component entry (...)`. Both
    // surfaces carry the role identity — the test asserts the identity,
    // not the message body, to stay forward-compatible.
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
}

/// Corrosion-present variant — asserts the helper imports the Rust
/// component staticlibs and the real (non-weak) entries land in the
/// binary. Ignored until Corrosion is added to the Phase 212 setup
/// tier (mirrors `phase212_d_workspace_metadata.rs`'s mixed-Corrosion
/// test).
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
fn threadx_linux_2_component_bringup_corrosion_imports_rust() {
    if require_test_prereqs().is_none() {
        nros_tests::skip!("prereqs missing (nros CLI / cmake)");
    }
    if !corrosion_available() {
        nros_tests::skip!("Corrosion not found via cmake --find-package");
    }

    let (_guard, root) = stage_fixture("multi_pkg_workspace_threadx");
    let app_src = root.join("threadx_app");
    let build_dir = app_src.join("build");
    let prefix_path = cmake_prefix_path_with_corrosion();

    let configure = Command::new("cmake")
        .env("CMAKE_PREFIX_PATH", &prefix_path)
        .args(["-S"])
        .arg(&app_src)
        .args(["-B"])
        .arg(&build_dir)
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

    let app = build_dir.join("threadx_app");
    let run = Command::new(&app).output().expect("spawn threadx_app");
    let out = String::from_utf8_lossy(&run.stdout);
    assert!(
        out.contains("[talker] component entry reached")
            && out.contains("[listener] component entry reached"),
        "Corrosion-imported Rust entries didn't fire:\n{out}"
    );
}

// =============================================================================
// Phase 212.H.4 — RISC-V 64 QEMU companion (configure-only)
// =============================================================================
//
// Sibling of `threadx_linux_2_component_bringup_builds_and_publishes`
// that exercises the SAME `nros_threadx_codegen_system(...)` helper +
// `cmake/platform/nano-ros-threadx.cmake` switch under
// `NANO_ROS_BOARD=riscv64-qemu`. The threadx-linux variant is
// host-linked + runnable; the rv64-qemu variant is a bare-metal cross-
// compile that needs an entry.s + linker script + ThreadX startup —
// out of scope for this codegen-surface audit.
//
// The test asserts the cmake CONFIGURE step on the existing
// `multi_pkg_workspace_threadx` fixture succeeds under
// `-DNANO_ROS_BOARD=riscv64-qemu` and emits the codegen artifacts
// (system_main.c, Cargo.toml stub, components cmake). The build step
// is deliberately skipped — `threadx_app/main.c` is host-shaped and
// wouldn't link without a bare-metal entry/linker script. A full
// rv64-qemu firmware fixture is a separate scope.

fn rv64_threadx_prereqs() -> Option<(PathBuf, PathBuf)> {
    use nros_tests::fixtures::threadx_riscv64::{
        is_netx_available, is_riscv_gcc_available, is_threadx_available,
    };
    if !require_test_prereqs().is_some() {
        return None;
    }
    if !is_threadx_available() {
        return None;
    }
    if !is_netx_available() {
        return None;
    }
    if !is_riscv_gcc_available() {
        return None;
    }
    let threadx_dir = PathBuf::from(std::env::var("THREADX_DIR").ok()?);
    let netx_dir = PathBuf::from(std::env::var("NETX_DIR").ok()?);
    Some((threadx_dir, netx_dir))
}

/// Phase 212.H.4 sibling — RISC-V 64 QEMU cross-platform codegen +
/// platform-module dispatch verification. Configure-only: the
/// `multi_pkg_workspace_threadx` fixture's `threadx_app/main.c` is
/// host-shaped and won't link bare-metal RV64 without an entry.s +
/// linker script (separate firmware fixture, out of H.4 scope).
#[test]
fn threadx_riscv64_qemu_2_component_bringup_builds() {
    let Some((threadx_dir, netx_dir)) = rv64_threadx_prereqs() else {
        nros_tests::skip!(
            "rv64 ThreadX prereqs missing (nros CLI / cmake / THREADX_DIR / NETX_DIR / riscv64-unknown-elf-gcc)"
        );
    };

    let board_config_dir =
        workspace_root().join("packages/boards/nros-board-threadx-qemu-riscv64/config");
    assert!(
        board_config_dir.is_dir(),
        "missing board config dir at {} — tx_user.h / nx_user.h / link.lds source",
        board_config_dir.display()
    );

    let (_guard, root) = stage_fixture("multi_pkg_workspace_threadx");
    let app_src = root.join("threadx_app");
    let build_dir = app_src.join("build");

    let configure = Command::new("cmake")
        .args(["-S"])
        .arg(&app_src)
        .args(["-B"])
        .arg(&build_dir)
        .arg("-DNANO_ROS_BOARD=riscv64-qemu")
        .arg(format!("-DTHREADX_DIR={}", threadx_dir.display()))
        .arg(format!("-DNETX_DIR={}", netx_dir.display()))
        .arg(format!(
            "-DTHREADX_CONFIG_DIR={}",
            board_config_dir.display()
        ))
        .arg(format!("-DNETX_CONFIG_DIR={}", board_config_dir.display()))
        .output()
        .expect("spawn cmake configure");
    assert!(
        configure.status.success(),
        "cmake configure (rv64-qemu) failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&configure.stdout),
        String::from_utf8_lossy(&configure.stderr)
    );

    // Same codegen surface as the threadx-linux variant — verifies the
    // platform module's codegen path is board-agnostic.
    let sys_main = build_dir.join("nros-system/system_main.c");
    let sys_cargo = build_dir.join("nros-system/Cargo.toml");
    let components_cmake = build_dir.join("nros_components.cmake");
    assert!(
        sys_main.is_file(),
        "missing {} after rv64-qemu configure",
        sys_main.display()
    );
    assert!(
        sys_cargo.is_file(),
        "missing {} after rv64-qemu configure",
        sys_cargo.display()
    );
    assert!(
        components_cmake.is_file(),
        "missing {} after rv64-qemu configure",
        components_cmake.display()
    );

    let sys_main_body = fs::read_to_string(&sys_main).expect("read system_main.c");
    assert!(
        sys_main_body.contains("__nros_component_talker_pkg_register")
            && sys_main_body.contains("__nros_component_listener_pkg_register"),
        "rv64-qemu system_main.c missing per-component register entries:\n{sys_main_body}"
    );
}
