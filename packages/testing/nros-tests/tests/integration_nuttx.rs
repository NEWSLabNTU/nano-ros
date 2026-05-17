//! Phase 139.6 — NuttX integration shell smoke test.
//!
//! NuttX consumers symlink `integrations/nuttx/` into
//! `apps/external/nano-ros/` under a configured NuttX checkout. The
//! full `make` involves the NuttX cross-toolchain — when that
//! toolchain is missing, this test skips cleanly via
//! `nros_tests::skip!`.

use std::{path::PathBuf, process::Command};

fn workspace_root() -> PathBuf {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .ancestors()
        .nth(3)
        .expect("workspace root above CARGO_MANIFEST_DIR")
        .to_path_buf()
}

fn have(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
fn nuttx_integration_shell_smoke() {
    // NuttX builds normally use `arm-none-eabi-gcc` for ARM
    // configurations. Use that as the toolchain discriminator.
    if !have("arm-none-eabi-gcc") {
        nros_tests::skip!(
            "arm-none-eabi-gcc not on PATH — install gcc-arm-none-eabi for NuttX builds"
        );
    }
    if std::env::var("NUTTX_DIR").is_err() {
        nros_tests::skip!(
            "NUTTX_DIR unset — point at a configured NuttX checkout (`apps/` sibling)"
        );
    }

    let root = workspace_root();
    let shell = root.join("integrations/nuttx");
    for f in &["Make.defs", "Makefile", "Kconfig", "CMakeLists.txt"] {
        let p = shell.join(f);
        assert!(p.exists(), "integrations/nuttx/{} missing", f);
    }

    let make_defs = std::fs::read_to_string(shell.join("Make.defs")).expect("read Make.defs");
    assert!(
        make_defs.contains("CONFIG_NROS"),
        "NuttX Make.defs must gate on CONFIG_NROS",
    );
    assert!(
        make_defs.contains("CONFIGURED_APPS"),
        "NuttX Make.defs must add to CONFIGURED_APPS",
    );

    let kconfig = std::fs::read_to_string(shell.join("Kconfig")).expect("read NuttX Kconfig");
    assert!(
        kconfig.contains("config NROS"),
        "NuttX Kconfig must declare config NROS",
    );

    let cmake_shell =
        std::fs::read_to_string(shell.join("CMakeLists.txt")).expect("read NuttX CMakeLists.txt");
    assert!(
        cmake_shell.contains("NANO_ROS_PLATFORM"),
        "NuttX shell CMake must set NANO_ROS_PLATFORM",
    );
    assert!(
        cmake_shell.contains("add_subdirectory"),
        "NuttX shell CMake must add_subdirectory the root CMake",
    );
}
