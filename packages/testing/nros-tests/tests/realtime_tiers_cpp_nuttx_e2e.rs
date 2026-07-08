//! phase-281 W3-nuttx — runtime E2E for RT-TIERS on embedded C++: the
//! `ws-realtime-cpp` workspace's NuttX (QEMU arm-virt) Entry. The FIRST full
//! nuttx link + runtime proof of the W3(nuttx) `NuttxBoard::run_tiers` seam
//! (commit 37cfaf728) — closes the C++ × nuttx cell of the RFC-0015 Model-1
//! convergence matrix.
//!
//! `demo_bringup/system.toml` declares two `[tiers.*]` (high: 10 ms ctrl timer;
//! low: 100 ms telem timer) with `[tiers.*.nuttx]` raw SCHED_FIFO priorities,
//! which flips the C++ codegen onto an `nros_app_main` calling
//! `NuttxBoard::run_tiers` → `nros_board_nuttx_run_tiers` (RFC-0015 Model 1):
//! one `pthread` per tier over ONE shared zenoh session — the boot thread runs
//! the `high` tier, a detached SCHED_FIFO pthread runs `low`; each tier opens a
//! borrowed executor over the same session with its `active_groups` filter.
//!
//! Delivery is observed CROSS-PROCESS (issue 0096 — the entry's own nodes can't
//! receive each other's in-image queries): the QEMU guest publishes /ctrl +
//! /telem, and two SEPARATE native `int32-sink` observers receive them through a
//! host zenoh router. The guest dials the router through the QEMU slirp gateway
//! (10.0.2.2 → host) at the baked port (17863); the observers dial
//! 127.0.0.1:17863. No TAP/bridge/root.
//!
//! Assertion mirrors the Zephyr sibling `realtime_tiers_cpp_zephyr_e2e`: anchor
//! on the SLOW tier (5 telem receives ≈ 0.5 s+ elapsed) and require the 10 ms
//! ctrl tier to have delivered strictly more — both tiers live, high runs faster.
//!
//! The fixture is built by `just nuttx build-examples` (or the workspace-fixtures
//! nuttx builder); this test skips cleanly when the NuttX kernel ELF is absent.
//!
//! Run with: `cargo nextest run -p nros-tests --test realtime_tiers_cpp_nuttx_e2e`

use nros_tests::fixtures::{
    ManagedProcess, QemuProcess, ZenohRouter, build_int32_sink,
    build_nuttx_workspace_cpp_realtime_entry, is_qemu_available, require_zenohd,
};
use std::{process::Command, time::Duration};

/// The router port baked into the C++ realtime nuttx entry's locator (see the
/// `workspace-cpp-nuttx-realtime` fixture's `NROS_ENTRY_LOCATOR = "tcp/10.0.2.2:17863"`).
const REALTIME_CPP_NUTTX_ENTRY_PORT: u16 = 17863;

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
fn realtime_tiers_cpp_nuttx_entry_schedules_high_and_low() {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    if !is_qemu_available() {
        nros_tests::skip!("qemu-system-arm not found");
    }

    let entry = build_nuttx_workspace_cpp_realtime_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("C++ nuttx realtime workspace entry not built: {e}"));

    // Router on the exact port the fixture's NROS_ENTRY_LOCATOR was baked with.
    // Listen on 0.0.0.0 so the QEMU slirp guest (10.0.2.2 gateway) can reach it.
    let router =
        ZenohRouter::start_on("0.0.0.0", REALTIME_CPP_NUTTX_ENTRY_PORT).unwrap_or_else(|e| {
            nros_tests::skip!("zenohd failed to start on {REALTIME_CPP_NUTTX_ENTRY_PORT}: {e}")
        });
    let _ = &router;
    let observer_locator = format!("tcp/127.0.0.1:{REALTIME_CPP_NUTTX_ENTRY_PORT}");

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
                "telem (low tier, 100 ms) never delivered 5 samples from the C++ NuttX \
                 entry — the low-tier pthread did not run (281 W3-nuttx / NuttxBoard::run_tiers)"
            )
        });
    // telem hitting its 5-sample anchor means ~0.5 s+ of BOTH tiers running; stop
    // the guest and drain everything each observer received. (#158 — no longer gate
    // ctrl on a sample COUNT: under scheduler/QEMU jitter a delivery-count target
    // could time out spuriously. The deterministic proof below reads the payload
    // counter, not the number of delivered samples.)
    qemu.kill();
    let ctrl_all = ctrl
        .wait_for_all_output(Duration::from_secs(3))
        .unwrap_or_default();
    let telem_all = format!(
        "{telem_out}{}",
        telem
            .wait_for_all_output(Duration::from_secs(3))
            .unwrap_or_default()
    );
    ctrl.kill();
    telem.kill();

    // Deterministic per-tier proof (#158): each tier publishes a MONOTONIC counter,
    // so its highest delivered value = how many times ITS OWN timer fired — robust
    // to zenoh delivery batching/drops that distort raw sample counts. The 10 ms
    // ctrl tier must outrun the 100 ms telem tier (~10×; assert a ≥3× margin).
    let telem_max = nros_tests::max_int_after(&telem_all, "Received:").unwrap_or(0);
    let ctrl_max = nros_tests::max_int_after(&ctrl_all, "Received:").unwrap_or(0);
    // Anchor already proved 5 low-tier samples; guard only against a parse-fail
    // (0-indexed counter ⇒ 5 samples = max value 4 — assert advancement, not a count).
    assert!(
        telem_max > 0,
        "low-tier /telem counter never advanced (max {telem_max}) — the low tier did not run \
         (281 W3-nuttx / NuttxBoard::run_tiers)"
    );
    assert!(
        ctrl_max >= 3 * telem_max,
        "high-tier /ctrl counter {ctrl_max} is not ≥3× the low-tier /telem counter \
         {telem_max} — the 10 ms tier is not outrunning the 100 ms tier \
         (281 W3-nuttx / NuttxBoard::run_tiers)"
    );
}
