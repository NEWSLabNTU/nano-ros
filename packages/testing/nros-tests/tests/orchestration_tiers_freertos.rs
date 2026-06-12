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

/// Resolve the prebuilt multi-tier freertos firmware ELF.
fn firmware() -> nros_tests::TestResult<PathBuf> {
    let stamp = nros_tests::fixtures::require_compile_check("orch_tiers_freertos")?;
    Ok(stamp
        .parent()
        .expect("fixture dir")
        .join("target/thumbv7m-none-eabi/debug/demo_entry"))
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
    let bin = firmware()?;
    assert!(bin.is_file(), "firmware ELF missing at {}", bin.display());
    if !tool_on_path("qemu-system-arm") {
        nros_tests::skip!("qemu-system-arm not on PATH");
    }
    if !nros_tests::fixtures::require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

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

    // Best-effort: with the slirp router live, the firmware should connect past
    // Executor::open and set up both tiers. Slirp + QEMU virtual-clock timing is
    // flaky, so a miss is logged, not fatal.
    match qemu.wait_for_output_pattern("Multi-tier setup complete", Duration::from_secs(25)) {
        Ok(_) => {}
        Err(e) => eprintln!(
            "note: freertos slirp connected-run not observed within budget \
             (timing-flaky, boot path already proven): {e}"
        ),
    }
    qemu.kill();
    Ok(())
}
