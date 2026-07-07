//! phase-281 W3c — runtime E2E for RT-TIERS on embedded C: the
//! `ws-realtime-c` workspace's Zephyr (native_sim) Entry. The FIRST full west
//! link + runtime proof of the W3a `ZephyrBoard::run_tiers` seam for a C node
//! (closes the C × zephyr cell of the RFC-0015 Model-1 convergence matrix,
//! completing W3 alongside the C++ sibling `realtime_tiers_cpp_zephyr_e2e`).
//!
//! `demo_bringup/system.toml` declares two `[tiers.*]` (high: 10 ms ctrl timer;
//! low: 100 ms telem timer) with `[tiers.*.zephyr]` raw priorities, which flips
//! the C codegen onto a plain `int main(void)` calling
//! `ZephyrBoard::run_tiers` → `nros_board_zephyr_run_tiers` (RFC-0015 Model 1):
//! one `k_thread` per tier over ONE shared zenoh session — the boot thread runs
//! the `high` tier, a static-pool thread runs `low`; each tier registers through
//! the same closure with its `active_groups` filter installed. The tier nodes
//! are C (`NROS_C_COMPONENT`, typed std_msgs Int32).
//!
//! Assertion mirrors the C++ sibling `realtime_tiers_cpp_zephyr_e2e`: two
//! `int32-sink` observers on `/ctrl` and `/telem`; anchor on the SLOW tier (5
//! telem receives ≈ 0.5 s+ elapsed) and require the 10 ms ctrl tier to have
//! delivered strictly more — both tiers live, high runs faster.
//!
//! Requires the west-lane fixture (`just zephyr build-fixtures`; skips when
//! `zephyr.exe` is absent) and the `int32-sink` fixture binary.
//!
//! Run with: `cargo nextest run -p nros-tests --test realtime_tiers_c_zephyr_e2e`

use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, ZephyrPlatform, ZephyrProcess, build_int32_sink,
    build_zephyr_workspace_c_realtime_entry,
};
use std::{process::Command, time::Duration};

/// The router port baked into the C realtime zephyr entry (the west lane's
/// `-DCONFIG_NROS_ZENOH_LOCATOR="tcp/127.0.0.1:17859"`).
const REALTIME_C_ZEPHYR_ENTRY_PORT: u16 = 17859;

/// Spawn an `int32-sink` on `topic` (prints `Received: <n>` per message).
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
fn realtime_tiers_c_zephyr_entry_schedules_high_and_low() {
    let entry = build_zephyr_workspace_c_realtime_entry().unwrap_or_else(|e| {
        nros_tests::skip!("C zephyr realtime workspace entry not built (west): {e}")
    });

    // Router on the exact port the fixture's CONFIG_NROS_ZENOH_LOCATOR was baked with.
    let router =
        ZenohRouter::start_on("127.0.0.1", REALTIME_C_ZEPHYR_ENTRY_PORT).unwrap_or_else(|e| {
            nros_tests::skip!("zenohd failed to start on {REALTIME_C_ZEPHYR_ENTRY_PORT}: {e}")
        });
    let locator = router.locator();

    // Observers first, so their subscriptions are live before the image publishes.
    let mut ctrl = spawn_listener("/ctrl", &locator);
    let mut telem = spawn_listener("/telem", &locator);

    // Boot the Zephyr native_sim image (runs until killed).
    let mut zephyr = ZephyrProcess::start(&entry, ZephyrPlatform::NativeSim)
        .unwrap_or_else(|e| panic!("boot zephyr native_sim: {e}"));

    // Anchor on the SLOW tier: 5 telem (100 ms) receives ≈ 0.5 s+ elapsed, so
    // the 10 ms ctrl tier must have delivered many more — both tiers live.
    let telem_out = telem
        .wait_for_output_count("Received:", 5, Duration::from_secs(60))
        .unwrap_or_else(|_| {
            zephyr.kill();
            ctrl.kill();
            telem.kill();
            panic!(
                "telem (low tier, 100 ms) never delivered 5 samples from the C Zephyr \
                 entry — the low-tier k_thread did not run (281 W3c / ZephyrBoard::run_tiers)"
            )
        });
    // Grab whatever the high tier has accumulated by now (it already has many).
    let ctrl_out = ctrl
        .wait_for_output_count("Received:", 1, Duration::from_secs(2))
        .unwrap_or_else(|_| {
            zephyr.kill();
            ctrl.kill();
            telem.kill();
            panic!(
                "ctrl (high tier, 10 ms) produced nothing — the boot tier was \
                 not scheduled (281 W3c / ZephyrBoard::run_tiers)"
            )
        });

    zephyr.kill();
    ctrl.kill();
    telem.kill();

    let telem_n = nros_tests::count_pattern(&telem_out, "Received:");
    let ctrl_n = nros_tests::count_pattern(&ctrl_out, "Received:");
    assert!(
        telem_n >= 5,
        "expected ≥5 low-tier /telem samples, got {telem_n}"
    );
    // 10 ms vs 100 ms ⇒ ~10×; require a clear margin over the anchor count so
    // the assertion proves the high tier actually runs FASTER, while staying
    // robust to zenoh delivery batching.
    assert!(
        ctrl_n > telem_n,
        "ctrl (10 ms tier) delivered {ctrl_n} ≤ telem's {telem_n} — the high \
         tier is not outrunning the low tier (281 W3c / ZephyrBoard::run_tiers)"
    );
}
