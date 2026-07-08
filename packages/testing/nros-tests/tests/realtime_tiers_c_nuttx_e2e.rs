//! phase-281 W3-nuttx — runtime E2E for RT-TIERS on embedded C: the
//! `ws-realtime-c` workspace's NuttX (QEMU arm-virt) Entry. The C sibling of
//! `realtime_tiers_cpp_nuttx_e2e`, proving the W3(nuttx) `NuttxBoard::run_tiers`
//! seam (commit 37cfaf728) on the pure-C lane — closes the C × nuttx cell of the
//! RFC-0015 Model-1 convergence matrix.
//!
//! `demo_bringup/system.toml` declares two `[tiers.*]` (high: 10 ms ctrl timer;
//! low: 100 ms telem timer) with `[tiers.*.nuttx]` raw SCHED_FIFO priorities,
//! which flips the C codegen onto an `nros_app_main` calling
//! `NuttxBoard::run_tiers` → `nros_board_nuttx_run_tiers` (RFC-0015 Model 1):
//! one `pthread` per tier over ONE shared zenoh session — the boot thread runs
//! the `high` tier, a detached SCHED_FIFO pthread runs `low`; each tier opens a
//! borrowed executor over the same session with its `active_groups` filter.
//!
//! Delivery is observed CROSS-PROCESS (issue 0096 — the entry's own nodes can't
//! receive each other's in-image queries): the QEMU guest publishes /ctrl +
//! /telem, and two SEPARATE native `int32-sink` observers receive them through a
//! host zenoh router. The guest dials the router through the QEMU slirp gateway
//! (10.0.2.2 → host) at the baked port (17864); the observers dial
//! 127.0.0.1:17864. No TAP/bridge/root.
//!
//! Assertion mirrors the C++ sibling `realtime_tiers_cpp_nuttx_e2e`: anchor on
//! the SLOW tier (5 telem receives ≈ 0.5 s+ elapsed) and require the 10 ms ctrl
//! tier to have delivered strictly more — both tiers live, high runs faster.
//!
//! The fixture is built by `just nuttx build-examples` (or the workspace-fixtures
//! nuttx builder); this test skips cleanly when the NuttX kernel ELF is absent.
//!
//! Run with: `cargo nextest run -p nros-tests --test realtime_tiers_c_nuttx_e2e`

use nros_tests::fixtures::{
    ManagedProcess, QemuProcess, ZenohRouter, build_int32_sink,
    build_nuttx_workspace_c_realtime_entry, is_qemu_available, require_zenohd,
};
use std::{process::Command, time::Duration};

/// The router port baked into the C realtime nuttx entry's locator (see the
/// `workspace-c-nuttx-realtime` fixture's `NROS_ENTRY_LOCATOR = "tcp/10.0.2.2:17864"`).
const REALTIME_C_NUTTX_ENTRY_PORT: u16 = 17864;

/// Spawn a native `int32-sink` observer on `topic` (prints `Received: <n>` per
/// message). Dials the host router at 127.0.0.1:<port>.
fn spawn_listener(topic: &'static str, locator: &str) -> ManagedProcess {
    let listener = build_int32_sink()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("int32-sink fixture not built: {e}"));
    let mut cmd = Command::new(listener);
    cmd.env("RUST_LOG", "info")
        .env("NROS_LOCATOR", locator)
        .env("NROS_SESSION_MODE", "client")
        .env("NROS_SUB_TOPIC", topic);
    let mut proc =
        ManagedProcess::spawn_command(cmd, topic).unwrap_or_else(|e| panic!("spawn {topic}: {e}"));
    proc.wait_for_output_pattern("Waiting for Int32", Duration::from_secs(10))
        .unwrap_or_else(|_| panic!("{topic} listener did not become ready"));
    proc
}

#[test]
fn realtime_tiers_c_nuttx_entry_schedules_high_and_low() {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    if !is_qemu_available() {
        nros_tests::skip!("qemu-system-arm not found");
    }

    let entry = build_nuttx_workspace_c_realtime_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("C nuttx realtime workspace entry not built: {e}"));

    // Router on the exact port the fixture's NROS_ENTRY_LOCATOR was baked with.
    // Listen on 0.0.0.0 so the QEMU slirp guest (10.0.2.2 gateway) can reach it.
    let router =
        ZenohRouter::start_on("0.0.0.0", REALTIME_C_NUTTX_ENTRY_PORT).unwrap_or_else(|e| {
            nros_tests::skip!("zenohd failed to start on {REALTIME_C_NUTTX_ENTRY_PORT}: {e}")
        });
    let _ = &router;
    let observer_locator = format!("tcp/127.0.0.1:{REALTIME_C_NUTTX_ENTRY_PORT}");

    // Observers first, so their subscriptions are live before the image publishes.
    let mut ctrl = spawn_listener("/ctrl", &observer_locator);
    let mut telem = spawn_listener("/telem", &observer_locator);

    // Boot the NuttX arm-virt image (runs until killed). eth0 is brought up by the
    // board FFI main before app_main → NuttxBoard::run_tiers.
    let mut qemu = QemuProcess::start_nuttx_virt(&entry, true)
        .unwrap_or_else(|e| panic!("boot NuttX QEMU: {e}"));

    // Anchor on the SLOW tier: 5 telem (100 ms) receives ≈ 0.5 s+ elapsed, so the
    // 10 ms ctrl tier must have delivered many more — both tiers live.
    let telem_out = telem
        .wait_for_output_count("Received:", 5, Duration::from_secs(90))
        .unwrap_or_else(|_| {
            qemu.kill();
            ctrl.kill();
            telem.kill();
            panic!(
                "telem (low tier, 100 ms) never delivered 5 samples from the C NuttX \
                 entry — the low-tier pthread did not run (281 W3-nuttx / NuttxBoard::run_tiers)"
            )
        });
    // The high tier (10 ms) is 10× the low (100 ms), so by the time telem hit its
    // 5-sample anchor (~0.5 s) ctrl has produced roughly 50. Wait for ctrl to reach
    // a count with a CLEAR margin over the anchor (15 ≈ 3×) — NOT just 1 sample.
    // Stopping at 1 captured `ctrl_out` too early (only ~1 match buffered), which
    // could tie or fall below telem's 5 and flake the `ctrl_n > telem_n` assertion.
    let ctrl_out = ctrl
        .wait_for_output_count("Received:", 15, Duration::from_secs(15))
        .unwrap_or_else(|_| {
            qemu.kill();
            ctrl.kill();
            telem.kill();
            panic!(
                "ctrl (high tier, 10 ms) did not reach 15 samples while telem (100 ms) \
                 held at its 5-sample anchor — the boot tier was not scheduled or is not \
                 outrunning the low tier (281 W3-nuttx / NuttxBoard::run_tiers)"
            )
        });

    qemu.kill();
    ctrl.kill();
    telem.kill();

    let telem_n = nros_tests::count_pattern(&telem_out, "Received:");
    let ctrl_n = nros_tests::count_pattern(&ctrl_out, "Received:");
    assert!(
        telem_n >= 5,
        "expected ≥5 low-tier /telem samples, got {telem_n}"
    );
    // 10 ms vs 100 ms ⇒ ~10×; require a clear margin over the anchor count so the
    // assertion proves the high tier actually runs FASTER, while staying robust to
    // zenoh delivery batching.
    assert!(
        ctrl_n > telem_n,
        "ctrl (10 ms tier) delivered {ctrl_n} ≤ telem's {telem_n} — the high tier is \
         not outrunning the low tier (281 W3-nuttx / NuttxBoard::run_tiers)"
    );
}
