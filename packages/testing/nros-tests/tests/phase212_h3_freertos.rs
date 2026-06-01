//! Phase 212.H.3 — FreeRTOS BSP cargo-native adapter test.
//!
//! Verifies the multi-pkg FreeRTOS fixture's `firmware/` bin crate
//! builds for `thumbv7m-none-eabi` when wired through
//! `freertos-qemu-mps2-an385-bsp`. The BSP is the Phase 212.H.3
//! adapter — its `build.rs` runs `nros codegen system` (or bakes the
//! equivalent fallback when 212.E isn't yet shipped) and compiles
//! `system_main.c`, replacing the per-example `app_config.h` baker
//! the live FreeRTOS examples still use.
//!
//! Skips cleanly when any of the FreeRTOS bring-up prerequisites are
//! missing (matches the gating pattern in `freertos_qemu.rs`):
//!
//! * `thumbv7m-none-eabi` Rust target installed
//! * `arm-none-eabi-gcc` toolchain
//! * `FREERTOS_DIR` + `LWIP_DIR` (resolved by `nros-build-paths` once
//!   `just freertos setup` has run, or by env override)
//!
//! Run with: `cargo test -p nros-tests --test phase212_h3_freertos`

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use nros_tests::fixtures::freertos::{is_arm_gcc_available, is_freertos_available, is_lwip_available};

fn workspace_root() -> PathBuf {
    nros_tests::project_root()
}

fn fixture_src() -> PathBuf {
    workspace_root().join("packages/testing/nros-tests/fixtures/multi_pkg_workspace_freertos")
}

/// Return true iff the `thumbv7m-none-eabi` rust target is installed
/// (mirrors `emulator.rs::require_arm_toolchain` but probes
/// `rustup` directly so the test doesn't depend on
/// `is_arm_toolchain_available`'s composite signal).
fn thumbv7m_target_installed() -> bool {
    let out = match Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
    {
        Ok(o) => o,
        Err(_) => return false,
    };
    if !out.status.success() {
        return false;
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .any(|l| l.trim() == "thumbv7m-none-eabi")
}

/// Copy the source fixture into a tempdir + rewrite `@NANO_ROS_ROOT@`
/// placeholders so the staged tree carries absolute `path =` deps.
fn stage_fixture() -> (tempfile::TempDir, PathBuf) {
    let src = fixture_src();
    let dst = tempfile::tempdir().expect("tempdir");
    copy_tree(&src, dst.path()).expect("copy fixture");
    let root_str = workspace_root()
        .to_str()
        .expect("workspace root is utf-8")
        .to_string();
    rewrite_placeholders(dst.path(), &root_str).expect("rewrite placeholders");
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

fn rewrite_placeholders(root: &Path, replacement: &str) -> std::io::Result<()> {
    for entry in walk(root)? {
        if !entry.is_file() {
            continue;
        }
        let Ok(text) = fs::read_to_string(&entry) else {
            continue;
        };
        if !text.contains("@NANO_ROS_ROOT@") {
            continue;
        }
        fs::write(&entry, text.replace("@NANO_ROS_ROOT@", replacement))?;
    }
    Ok(())
}

fn walk(root: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(p) = stack.pop() {
        if p.is_dir() {
            for e in fs::read_dir(&p)? {
                stack.push(e?.path());
            }
        } else {
            out.push(p);
        }
    }
    Ok(out)
}

fn require_freertos_prereqs() -> Option<&'static str> {
    if !thumbv7m_target_installed() {
        return Some("thumbv7m-none-eabi target not installed");
    }
    if !is_arm_gcc_available() {
        return Some("arm-none-eabi-gcc not found");
    }
    if !is_freertos_available() {
        return Some("FREERTOS_DIR not set or invalid — run `just freertos setup`");
    }
    if !is_lwip_available() {
        return Some("LWIP_DIR not set or invalid — run `just freertos setup`");
    }
    None
}

#[test]
fn freertos_qemu_mps2_an385_2_component_bringup_builds() {
    if let Some(reason) = require_freertos_prereqs() {
        nros_tests::skip!("{reason}");
    }

    let (_guard, root) = stage_fixture();
    let firmware = root.join("firmware");
    let bringup = root.join("src/demo_bringup");

    // Point the BSP's build.rs at the fixture's bringup spec so the
    // generated `nros_config_generated.h` carries the fixture's
    // domain_id / rmw / components / locator.
    // Mirror `just/sdk-env.just`'s defaults — the underlying
    // `nros-board-freertos` + dep chain panic without them. Direct
    // `cargo build` (no `just` wrapper) inherits zero of them.
    let root = workspace_root();
    let env_pairs: [(&str, PathBuf); 2] = [
        ("NROS_PLATFORM_FREERTOS_SRC", root.join("packages/core/nros-platform-freertos/src")),
        ("NROS_PLATFORM_CFFI_INCLUDE", root.join("packages/core/nros-platform-cffi/include")),
    ];

    let mut cmd = Command::new("cargo");
    cmd.args(["build", "--target", "thumbv7m-none-eabi", "-p", "firmware"])
        .env("NROS_SYSTEM_TOML", bringup.join("system.toml"))
        .env("NROS_BRINGUP_DIR", &bringup)
        .current_dir(&firmware);
    for (k, v) in &env_pairs {
        cmd.env(k, v);
    }
    let build = cmd.output().expect("spawn cargo build");

    assert!(
        build.status.success(),
        "cargo build --target thumbv7m-none-eabi -p firmware failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );

    // Confirm the BSP's build.rs actually emitted the baked tree.
    // The artifact lives under firmware/target/.../build/<bsp>-<hash>/out/nros-system/.
    let target_dir = firmware.join("target/thumbv7m-none-eabi/debug/build");
    let mut found_header = false;
    if target_dir.is_dir() {
        for e in walk(&target_dir).unwrap_or_default() {
            if e.file_name().is_some_and(|n| n == "nros_config_generated.h") {
                found_header = true;
                break;
            }
        }
    }
    assert!(
        found_header,
        "BSP build.rs did not emit nros_config_generated.h under {}",
        target_dir.display()
    );
}
