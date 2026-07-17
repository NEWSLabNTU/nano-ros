//! phase-281 W3-nuttx — runtime E2E for RT-TIERS on embedded **Rust**: the
//! `ws-realtime-rust` workspace's NuttX (QEMU arm-virt) Entry. The Rust sibling
//! of `realtime_tiers_c_nuttx_e2e` / `realtime_tiers_cpp_nuttx_e2e`, and the
//! LAST cell of the RFC-0015 Model-1 convergence matrix — closing it makes all
//! 12 lang×platform cells proven.
//!
//! `demo_bringup/system.toml` declares two `[tiers.*]` (high: 10 ms ctrl timer;
//! low: 100 ms telem timer) with `[tiers.*.nuttx]` raw SCHED_FIFO priorities,
//! which flips the macro's generic OwnedSpin arm onto
//! `<QemuArmVirt>::run_tiers` → `nros_board_nuttx::run_tiers` (RFC-0015 Model 1):
//! one `std::thread` per tier over ONE shared zenoh session (NuttX ships `std`
//! and its zenoh-pico build sets `Z_FEATURE_MULTI_THREAD = 1`, so this mirrors
//! the native posix path rather than the Zephyr k_thread shim) — the boot thread
//! runs the `high` tier, a spawned thread runs `low`; each tier opens a borrowed
//! executor over the same session with its `active_groups` filter installed.
//!
//! Delivery is observed CROSS-PROCESS (issue 0096 — the entry's own nodes can't
//! receive each other's in-image queries): the QEMU guest publishes /ctrl +
//! /telem, and two SEPARATE native `int32-sink` observers receive them through a
//! host zenoh router. The guest dials the router through the QEMU slirp gateway
//! (10.0.2.2 → host) at the baked port (17866); the observers dial
//! 127.0.0.1:17866. No TAP/bridge/root.
//!
//! Assertion mirrors the C/C++ siblings: anchor on the SLOW tier (5 telem
//! receives ≈ 0.5 s+ elapsed) and require the 10 ms ctrl tier to have delivered
//! strictly more — both tiers live, high runs faster.
//!
//! The fixture is built by `just nuttx build-examples` (the cargo cross lane);
//! this test skips cleanly when the NuttX kernel ELF / zenohd / qemu are absent.
//!
//! Run with: `cargo nextest run -p nros-tests --test realtime_tiers_rust_nuttx_e2e`

use nros_tests::fixtures::{
    ManagedProcess, QemuProcess, ZenohRouter, build_int32_sink,
    build_nuttx_workspace_rust_realtime_entry, is_qemu_available, require_zenohd,
};
use std::{process::Command, time::Duration};

/// The router port baked into the Rust realtime nuttx entry's locator (see the
/// `workspace-rust-nuttx-realtime` fixture's `NROS_LOCATOR = "tcp/10.0.2.2:17866"`).
const REALTIME_RUST_NUTTX_ENTRY_PORT: u16 = 17866;

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
fn realtime_tiers_rust_nuttx_entry_schedules_high_and_low() {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    if !is_qemu_available() {
        nros_tests::skip!("qemu-system-arm not found");
    }

    let entry = build_nuttx_workspace_rust_realtime_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| {
            nros_tests::skip!("Rust nuttx realtime workspace entry not built: {e}")
        });

    // Router on the exact port the fixture's NROS_LOCATOR was baked with. Listen
    // on 0.0.0.0 so the QEMU slirp guest (10.0.2.2 gateway) can reach it.
    let router =
        ZenohRouter::start_on("0.0.0.0", REALTIME_RUST_NUTTX_ENTRY_PORT).unwrap_or_else(|e| {
            nros_tests::skip!("zenohd failed to start on {REALTIME_RUST_NUTTX_ENTRY_PORT}: {e}")
        });
    let _ = &router;
    let observer_locator = format!("tcp/127.0.0.1:{REALTIME_RUST_NUTTX_ENTRY_PORT}");

    // Observers first, so their subscriptions are live before the image publishes.
    let mut ctrl = spawn_listener("/ctrl", &observer_locator);
    let mut telem = spawn_listener("/telem", &observer_locator);

    // Boot the NuttX arm-virt image (runs until killed). eth0 is brought up by
    // `entry_net_init` before `QemuArmVirt::run_tiers` opens the session.
    let mut qemu = QemuProcess::start_nuttx_virt(&entry, true)
        .unwrap_or_else(|e| panic!("boot NuttX QEMU: {e}"));

    // Anchor on the SLOW tier: 5 telem (100 ms) receives ≈ 0.5 s+ elapsed, so the
    // 10 ms ctrl tier must have delivered many more — both tiers live.
    let telem_out = telem
        .wait_for_output_count(
            nros_tests::output::INT32_LISTENER_LOG_PREFIX,
            5,
            Duration::from_secs(90),
        )
        .unwrap_or_else(|_| {
            qemu.kill();
            ctrl.kill();
            telem.kill();
            panic!(
                "telem (low tier, 100 ms) never delivered 5 samples from the Rust NuttX \
                 entry — the low-tier thread did not run (281 W3-nuttx / QemuArmVirt::run_tiers)"
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
    let telem_max =
        nros_tests::max_int_after(&telem_all, nros_tests::output::INT32_LISTENER_LOG_PREFIX)
            .unwrap_or(0);
    let ctrl_max =
        nros_tests::max_int_after(&ctrl_all, nros_tests::output::INT32_LISTENER_LOG_PREFIX)
            .unwrap_or(0);
    // Anchor already proved 5 low-tier samples; guard only against a parse-fail
    // (0-indexed counter ⇒ 5 samples = max value 4 — assert advancement, not a count).
    assert!(
        telem_max > 0,
        "low-tier /telem counter never advanced (max {telem_max}) — the low tier did not run \
         (281 W3-nuttx / QemuArmVirt::run_tiers)"
    );
    assert!(
        ctrl_max >= 3 * telem_max,
        "high-tier /ctrl counter {ctrl_max} is not ≥3× the low-tier /telem counter \
         {telem_max} — the 10 ms tier is not outrunning the 100 ms tier \
         (281 W3-nuttx / QemuArmVirt::run_tiers)"
    );
}
