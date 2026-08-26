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

use nros_tests::{
    alloc::port_of,
    matrix::{Lang, PlatformId, Workload},
};
use std::{path::PathBuf, process::Command, time::Duration};

/// Issue 0342 — this cell's allocator slot. The firmware bakes the port into
/// its deploy locator (`fixtures/orchestration_tiers_freertos/entry/Cargo.toml`)
/// and the host router must listen on the same one; [`assert_fixture_port`]
/// keeps the two from drifting.
const ROUTER_PORT: u16 = port_of(
    PlatformId::FreertosMps2,
    Lang::Rust,
    Workload::RealtimeTiers,
);

/// Issue 0342 — fail loudly if the fixture's baked locator and [`ROUTER_PORT`]
/// disagree.
///
/// A `Cargo.toml` literal cannot call the allocator, so this pairing is a hand
/// mirror — the class that silently rots. Checking it here costs a file read and
/// turns "the firmware dials a port nobody is listening on" (which looks like a
/// network or timing failure) into a named mismatch.
fn assert_fixture_port() {
    let manifest = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/fixtures/orchestration_tiers_freertos/entry/Cargo.toml"
    );
    let toml = std::fs::read_to_string(manifest)
        .unwrap_or_else(|e| panic!("read fixture manifest {manifest}: {e}"));
    let expected = format!("locator = \"tcp/10.0.2.2:{ROUTER_PORT}\"");
    assert!(
        toml.contains(&expected),
        "fixture's baked locator does not match the allocator slot for this cell \
         (`port_of(FreertosMps2, Rust, RealtimeTiers)` = {ROUTER_PORT}); expected a line \
         `{expected}` in {manifest}"
    );
}

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

/// Resolve the prebuilt **release** firmware ELF from the staged fixture tree.
///
/// The connected run opens a real zenoh-pico session over slirp on the emulated
/// Cortex-M3; a debug-profile zenoh-pico is far too slow to finish the session
/// handshake within the test budget (boots to `Network ready.` but never
/// connects). The build stage (`compile-check-fixtures.sh`, the
/// `orch_tiers_freertos` cross-build with `debug,release` profiles) now emits
/// BOTH the debug and the release ELF in the same staged tree — mirroring the
/// C++ `CMAKE_BUILD_TYPE=Release` path — so this just resolves the prebuilt
/// release binary. No cargo at run time (issue 0034 / 0041, phase-281 W1).
fn firmware_release() -> nros_tests::TestResult<PathBuf> {
    let stamp = nros_tests::fixtures::require_compile_check("orch_tiers_freertos")?;
    Ok(stamp
        .parent()
        .expect("fixture dir")
        .join("target/thumbv7m-none-eabi/release/demo_entry"))
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
    //
    // Issue 0342 — through the `qemu` interpreter, not a hand-rolled
    // `Command::new("timeout").args(["qemu-system-arm", …])`. The sibling test
    // below already used it, so the bypass was never a capability gap: it just
    // missed what the interpreter centralises — the `-icount shift=auto`
    // convention (docs/reference/qemu-icount.md), boot-deadline handling and log
    // capture.
    let mut qemu = nros_tests::qemu::QemuProcess::start_mps2_an385(&bin)
        .expect("boot multi-tier freertos firmware on QEMU");
    let combined = qemu
        .wait_for_output_pattern("Network ready.", Duration::from_secs(15))
        .unwrap_or_else(|e| {
            panic!("run_tiers boot bringup did not complete the network init: {e}")
        });
    assert!(
        combined.contains("nros FreeRTOS Platform (multi-tier)"),
        "QEMU boot did not reach run_tiers_entry (no multi-tier banner).\noutput:\n{combined}",
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

    // Host router on the cell's allocator slot — the fixture's deploy overlay
    // points the firmware at `tcp/10.0.2.2:<ROUTER_PORT>` (the slirp host
    // alias). Issue 0342: this was a bare `7447`, the only such literal among 14
    // `start_slirp` call sites, and it sat inside another platform's window.
    assert_fixture_port();
    let _router =
        nros_tests::fixtures::or_skip(nros_tests::fixtures::ZenohRouter::start_slirp(ROUTER_PORT));

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
    let connected = qemu.collect_until("Multi-tier setup complete", Duration::from_secs(25));
    qemu.kill();
    assert!(
        connected.contains("Multi-tier setup complete"),
        "multi-tier firmware did not reach `Multi-tier setup complete` over slirp \
         (Executor::open / per-tier setup failed).\noutput:\n{connected}",
    );
    Ok(())
}
