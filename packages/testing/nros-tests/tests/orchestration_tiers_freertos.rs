//! Phase 228.E.2 — multi-tier FreeRTOS `run_tiers` build + QEMU-boot E2E.
//!
//! Stages the `orchestration_tiers_freertos` fixture (an `nros::main!(launch=…)`
//! Entry pkg with `deploy = "freertos"` + a 2-tier `system.toml`), rewrites
//! `@NANO_ROS_ROOT@`, `cargo build`s for `thumbv7m-none-eabi`, then boots the
//! firmware on QEMU (mps2-an385).
//!
//! Because the system declares `[tiers.*]`, the macro emits
//! `<Mps2An385>::run_tiers(TIERS, run_plan)` (228.G) which routes to the
//! FreeRTOS per-tier entry (228.E.2). The build proves the
//! macro→run_tiers→kernel-link path; the QEMU boot proves `run_tiers_entry`
//! executes on the device — it prints the unique `(multi-tier)` banner, brings
//! up the network, then reaches the boot-tier `Executor::open` (which fails on
//! the absent router, exactly the entry-poc/native-G.6 lifecycle proof — no
//! zenohd needed).
//!
//! Skips cleanly when the FreeRTOS bring-up prerequisites are missing.
//!
//! Run with: `cargo test -p nros-tests --test orchestration_tiers_freertos`

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn workspace_root() -> PathBuf {
    nros_tests::project_root()
}

fn fixture_src() -> PathBuf {
    workspace_root().join("packages/testing/nros-tests/fixtures/orchestration_tiers_freertos")
}

fn tool_on_path(tool: &str) -> bool {
    Command::new(tool)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn thumbv7m_installed() -> bool {
    Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("thumbv7m-none-eabi"))
        .unwrap_or(false)
}

/// Returns the FreeRTOS SDK env the build needs, or `None` if a prerequisite is
/// missing (→ skip).
fn freertos_env() -> Option<Vec<(&'static str, PathBuf)>> {
    if !thumbv7m_installed()
        || !tool_on_path("arm-none-eabi-gcc")
        || !tool_on_path("qemu-system-arm")
    {
        return None;
    }
    let root = workspace_root();
    let kernel = root.join("third-party/freertos/kernel");
    let lwip = root.join("third-party/freertos/lwip");
    if !kernel.is_dir() || !lwip.is_dir() {
        return None;
    }
    Some(vec![
        ("FREERTOS_DIR", kernel),
        ("LWIP_DIR", lwip),
        (
            "FREERTOS_CONFIG_DIR",
            root.join("packages/boards/nros-board-mps2-an385-freertos/config"),
        ),
        (
            "NROS_PLATFORM_CFFI_INCLUDE",
            root.join("packages/core/nros-platform-cffi/include"),
        ),
        (
            "NROS_PLATFORM_FREERTOS_SRC",
            root.join("packages/core/nros-platform-freertos/src"),
        ),
    ])
}

fn stage_fixture() -> (tempfile::TempDir, PathBuf) {
    let dst = tempfile::tempdir().expect("tempdir");
    copy_tree(&fixture_src(), dst.path()).expect("copy fixture");
    let root_str = workspace_root().to_str().expect("utf-8").to_string();
    let mut stack = vec![dst.path().to_path_buf()];
    while let Some(p) = stack.pop() {
        if p.is_dir() {
            for e in fs::read_dir(&p).expect("read_dir") {
                stack.push(e.expect("entry").path());
            }
        } else if let Ok(text) = fs::read_to_string(&p) {
            if text.contains("@NANO_ROS_ROOT@") {
                fs::write(&p, text.replace("@NANO_ROS_ROOT@", &root_str)).expect("rewrite");
            }
        }
    }
    let root = dst.path().to_path_buf();
    (dst, root)
}

fn copy_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&from, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

#[test]
fn multi_tier_freertos_firmware_builds_and_boots_run_tiers() {
    let Some(env) = freertos_env() else {
        nros_tests::skip!("FreeRTOS prereqs missing (thumbv7m / arm-gcc / qemu / kernel / lwip)");
    };
    let (_guard, root) = stage_fixture();

    // Build the multi-tier firmware for thumbv7m. Success proves the macro
    // emitted `run_tiers` for the freertos deploy AND it links with the kernel.
    let mut build = Command::new("cargo");
    build
        .args([
            "build",
            "-p",
            "demo_entry",
            "--target",
            "thumbv7m-none-eabi",
        ])
        .current_dir(&root);
    for (k, v) in &env {
        build.env(k, v);
    }
    let out = build.output().expect("spawn cargo build");
    assert!(
        out.status.success(),
        "multi-tier freertos firmware failed to build.\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );
    let bin = root.join("target/thumbv7m-none-eabi/debug/demo_entry");
    assert!(
        bin.is_file(),
        "firmware ELF not produced at {}",
        bin.display()
    );

    // Boot it on QEMU (no router). `run_tiers_entry` prints the unique
    // `(multi-tier)` banner + brings up the network before the boot-tier
    // Executor::open fails — proving the run_tiers path executes on device.
    let qemu = Command::new("timeout")
        .args([
            "10",
            "qemu-system-arm",
            "-cpu",
            "cortex-m3",
            "-machine",
            "mps2-an385",
            "-nographic",
            "-semihosting-config",
            "enable=on,target=native",
            "-kernel",
        ])
        .arg(&bin)
        .output()
        .expect("spawn qemu");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&qemu.stdout),
        String::from_utf8_lossy(&qemu.stderr)
    );
    assert!(
        combined.contains("nros FreeRTOS Platform (multi-tier)"),
        "QEMU boot did not reach run_tiers_entry (no multi-tier banner).\noutput:\n{combined}",
    );
    assert!(
        combined.contains("Network ready."),
        "run_tiers boot bringup did not complete the network init.\noutput:\n{combined}",
    );
}
