//! #199 follow-up — runtime E2E for RT-TIERS on embedded C, NuttX QEMU
//! **rv-virt** (riscv32): the `ws-realtime-c` workspace's riscv Entry
//! (`src/riscv_nuttx_entry`). The riscv sibling of `realtime_tiers_c_nuttx_e2e`
//! (arm-virt) and the C sibling of `realtime_tiers_riscv_nuttx_e2e` (rust) —
//! this covers the (c, nuttx-riscv) cell of the RFC-0015 Model-1 convergence
//! matrix (`exec_model_matrix.rs`).
//!
//! `demo_bringup/system.toml` declares two `[tiers.*]` (high: 10 ms ctrl timer;
//! low: 100 ms telem timer) with `[tiers.*.nuttx]` raw SCHED_FIFO priorities
//! (the tier table keys on the RTOS, shared with the arm board), which flips
//! the C codegen onto an `nros_app_main` calling `NuttxBoard::run_tiers`
//! (RFC-0015 Model 1): one `pthread` per tier over ONE shared zenoh session.
//!
//! Delivery is observed CROSS-PROCESS (issue 0096): the QEMU guest publishes
//! /ctrl + /telem, and two SEPARATE native `int32-sink` observers receive them
//! through a host zenoh router. The guest dials the router through the QEMU
//! slirp gateway (10.0.2.2 → host) at the baked port (17869 — the arm C
//! sibling uses 17864, the rust riscv sibling 17867); the observers dial
//! 127.0.0.1:17869. No TAP/bridge/root.
//!
//! Assertion mirrors the siblings (#158 deterministic per-tier proof): anchor
//! on the SLOW tier (5 telem receives), then compare the tiers' MONOTONIC
//! payload counters — the 10 ms ctrl tier must outrun the 100 ms telem tier by
//! ≥3×.
//!
//! The fixture is built by `just nuttx build-riscv-c-workspaces` (provisions
//! the rv-virt kernel export); this test skips cleanly when the entry ELF /
//! zenohd / qemu-system-riscv32 are absent.
//!
//! Run with: `cargo nextest run -p nros-tests --test realtime_tiers_c_riscv_nuttx_e2e`

use nros_tests::fixtures::{
    ManagedProcess, QemuProcess, ZenohRouter, build_int32_sink,
    build_nuttx_riscv_workspace_c_realtime_entry, require_zenohd,
};
use std::{process::Command, time::Duration};

/// The router port baked into the C riscv realtime nuttx entry's locator (see the
/// `workspace-c-nuttx-riscv-realtime` fixture's `NROS_ENTRY_LOCATOR = "tcp/10.0.2.2:17869"`).
const REALTIME_C_RISCV_NUTTX_ENTRY_PORT: u16 = 17869;

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
fn realtime_tiers_c_riscv_nuttx_entry_schedules_high_and_low() {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    if !nros_tests::esp32::is_qemu_riscv32_available() {
        nros_tests::skip!("qemu-system-riscv32 not found");
    }

    let entry = build_nuttx_riscv_workspace_c_realtime_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| {
            nros_tests::skip!("C riscv-nuttx realtime workspace entry not built: {e}")
        });

    // Router on the exact port the fixture's NROS_ENTRY_LOCATOR was baked with.
    // Listen on 0.0.0.0 so the QEMU slirp guest (10.0.2.2 gateway) can reach it.
    let router = ZenohRouter::start_on("0.0.0.0", REALTIME_C_RISCV_NUTTX_ENTRY_PORT)
        .unwrap_or_else(|e| {
            nros_tests::skip!("zenohd failed to start on {REALTIME_C_RISCV_NUTTX_ENTRY_PORT}: {e}")
        });
    let _ = &router;
    let observer_locator = format!("tcp/127.0.0.1:{REALTIME_C_RISCV_NUTTX_ENTRY_PORT}");

    // Observers first, so their subscriptions are live before the image publishes.
    let mut ctrl = spawn_listener("/ctrl", &observer_locator);
    let mut telem = spawn_listener("/telem", &observer_locator);

    // Boot the NuttX rv-virt image (runs until killed). eth0 is brought up by the
    // board FFI main before app_main → NuttxBoard::run_tiers.
    let mut qemu = QemuProcess::start_nuttx_riscv(&entry, true)
        .unwrap_or_else(|e| panic!("boot NuttX rv-virt QEMU: {e}"));

    // Anchor on the SLOW tier: 5 telem (100 ms) receives ≈ 0.5 s+ elapsed, so the
    // 10 ms ctrl tier must have delivered many more — both tiers live.
    let telem_out = telem
        .wait_for_output_count("Received:", 5, Duration::from_secs(90))
        .unwrap_or_else(|_| {
            qemu.kill();
            ctrl.kill();
            telem.kill();
            panic!(
                "telem (low tier, 100 ms) never delivered 5 samples from the C riscv NuttX \
                 entry — the low-tier pthread did not run (#199 follow-up / NuttxBoard::run_tiers)"
            )
        });
    // Stop the guest and drain everything each observer received (#158 — the
    // deterministic proof reads the payload counter, not raw sample counts).
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

    // Deterministic per-tier proof (#158): each tier publishes a MONOTONIC
    // counter, so its highest delivered value = how many times ITS OWN timer
    // fired. The 10 ms ctrl tier must outrun the 100 ms telem tier (~10×;
    // assert a ≥3× margin).
    let telem_max = nros_tests::max_int_after(&telem_all, "Received:").unwrap_or(0);
    let ctrl_max = nros_tests::max_int_after(&ctrl_all, "Received:").unwrap_or(0);
    assert!(
        telem_max > 0,
        "low-tier /telem counter never advanced (max {telem_max}) — the low tier did not run \
         (#199 follow-up / NuttxBoard::run_tiers)"
    );
    assert!(
        ctrl_max >= 3 * telem_max,
        "high-tier /ctrl counter {ctrl_max} is not ≥3× the low-tier /telem counter \
         {telem_max} — the 10 ms tier is not outrunning the 100 ms tier \
         (#199 follow-up / NuttxBoard::run_tiers)"
    );
}
