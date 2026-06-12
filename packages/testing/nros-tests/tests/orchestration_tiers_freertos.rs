//! Multi-tier `nros::main!()` on FreeRTOS QEMU (mps2-an385) — `run_tiers`
//! executes on device (Phase 228.G).
//!
//! The fixture's `system.toml` declares `[tiers.*]`, so the macro emits
//! `<Mps2An385>::run_tiers(TIERS, run_plan)`. Booting the firmware proves the
//! emit links with the kernel and the run_tiers path runs on device (the
//! `(multi-tier)` banner + network bringup). A second test brings up a host
//! zenohd reachable over slirp and best-effort confirms the connected per-tier
//! run.
//!
//! The thumbv7m firmware cross build runs in the **build stage** — the
//! `orch_tiers_freertos` cross-build fixture (`compile-check-fixtures.sh`, run
//! by `build-test-fixtures`) builds `demo_entry`. These tests boot the prebuilt
//! ELF in QEMU instead of running cargo at run time (issue 0034 / 0041). Fixture
//! absent (thumbv7m / arm-gcc / FreeRTOS+lwIP not provisioned) → tier-aware
//! skip/fail via the resolver.

use std::{path::PathBuf, process::Command, time::Duration};

fn tool_on_path(tool: &str) -> bool {
    Command::new(tool)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Resolve the prebuilt multi-tier freertos firmware ELF (debug — the build
/// stage's default profile). Used by the boot-only test, where speed of
/// `Executor::open` is irrelevant.
fn firmware() -> nros_tests::TestResult<PathBuf> {
    let stamp = nros_tests::fixtures::require_compile_check("orch_tiers_freertos")?;
    Ok(stamp
        .parent()
        .expect("fixture dir")
        .join("target/thumbv7m-none-eabi/debug/demo_entry"))
}

/// Build + resolve the **release** firmware in the already-staged fixture tree.
///
/// The connected run opens a real zenoh-pico session over slirp on the emulated
/// Cortex-M3; a debug-profile zenoh-pico is far too slow to finish the session
/// handshake within the test budget (boots to `Network ready.` but never
/// connects). The build stage produces only the debug ELF, so build release here
/// in the same staged tree (the firmware C glue env mirrors
/// `compile-check-fixtures.sh`).
fn firmware_release() -> nros_tests::TestResult<PathBuf> {
    let stamp = nros_tests::fixtures::require_compile_check("orch_tiers_freertos")?;
    let staged = stamp.parent().expect("fixture dir").to_path_buf();
    let root = nros_tests::project_root();
    let out = Command::new("cargo")
        .args([
            "build",
            "--release",
            "--target",
            "thumbv7m-none-eabi",
            "-p",
            "demo_entry",
        ])
        .current_dir(&staged)
        .env(
            "NROS_PLATFORM_FREERTOS_SRC",
            root.join("packages/core/nros-platform-freertos/src"),
        )
        .env(
            // phase-241 B.2 — canonical platform headers live in nros-platform-api.
            "NROS_PLATFORM_CFFI_INCLUDE",
            root.join("packages/core/nros-platform-api/include"),
        )
        .output()
        .map_err(|e| {
            nros_tests::TestError::BuildFailed(format!("spawn cargo build --release: {e}"))
        })?;
    if !out.status.success() {
        return Err(nros_tests::TestError::BuildFailed(format!(
            "release cross build failed in {}.\nstdout:\n{}\nstderr:\n{}",
            staged.display(),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        )));
    }
    Ok(staged.join("target/thumbv7m-none-eabi/release/demo_entry"))
}

#[test]
fn multi_tier_freertos_firmware_builds_and_boots_run_tiers() -> nros_tests::TestResult<()> {
    let bin = firmware()?;
    assert!(bin.is_file(), "firmware ELF missing at {}", bin.display());
    if !tool_on_path("qemu-system-arm") {
        nros_tests::skip!("qemu-system-arm not on PATH");
    }

    // Boot on QEMU (no router). `run_tiers_entry` prints the unique
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
    Ok(())
}

#[test]
fn multi_tier_freertos_firmware_connects_over_slirp_and_runs_tiers() -> nros_tests::TestResult<()> {
    if !tool_on_path("qemu-system-arm") {
        nros_tests::skip!("qemu-system-arm not on PATH");
    }
    if !nros_tests::fixtures::require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    // Release fixture — debug zenoh-pico on the emulated M3 is too slow to finish
    // the session handshake in budget (see `firmware_release`).
    let bin = firmware_release()?;
    assert!(bin.is_file(), "firmware ELF missing at {}", bin.display());

    // Host router on 0.0.0.0:7447 — the fixture's deploy overlay points the
    // firmware at tcp/10.0.2.2:7447 (the slirp host alias).
    let _router =
        nros_tests::fixtures::ZenohRouter::start_slirp(7447).expect("start slirp zenohd router");

    let mut qemu = nros_tests::qemu::QemuProcess::start_mps2_an385_networked(&bin)
        .expect("boot multi-tier freertos firmware on QEMU");

    let boot = qemu
        .wait_for_output_pattern("Network ready.", Duration::from_secs(15))
        .expect("firmware did not reach network bringup");
    assert!(
        boot.contains("nros FreeRTOS Platform (multi-tier)"),
        "QEMU boot did not reach run_tiers_entry (no multi-tier banner).\noutput:\n{boot}",
    );

    // Connected run — ASSERTED (was best-effort) now that #48 is fixed: the zenoh
    // RMW backend is linked + registered (cause 2) AND `Mps2An385::run_tiers`
    // threads the deploy overlay (cause 1, multi-tier path) so the firmware dials
    // the reachable slirp locator. With adequate tier stacks (system.toml 64 KiB,
    // not the old 8/4 KiB that overflowed once the connect succeeded), both tiers
    // set up and the boot tier enters its spin loop.
    let connected = qemu
        .wait_for_output_pattern("Multi-tier setup complete", Duration::from_secs(25))
        .unwrap_or_default();
    qemu.kill();
    assert!(
        connected.contains("Multi-tier setup complete"),
        "multi-tier firmware did not reach `Multi-tier setup complete` over slirp \
         (Executor::open / per-tier setup failed).\noutput:\n{connected}",
    );
    Ok(())
}
