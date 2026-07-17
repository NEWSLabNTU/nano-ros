//! phase-279 W1 — MEASUREMENT SCRATCH (not a CI gate). Quantifies the Zephyr tx
//! throughput ceiling (#145): boots the ws-realtime-rust native_sim (ctrl 100 Hz
//! and telem 10 Hz over ONE zenoh session), runs a FIXED window, and prints the
//! per-tier and total received msg/s. Compare the numbers across
//! `CONFIG_NROS_ZENOH_SOCKET_TIMEOUT_MS` values (rebuild the entry between runs).
//!
//! Run: `cargo nextest run -p nros-tests --test w1_zephyr_tx_throughput_measure --no-capture`

use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, ZephyrPlatform, ZephyrProcess, build_int32_sink,
    build_zephyr_workspace_rust_realtime_entry,
};
use std::{process::Command, time::Duration};

const REALTIME_ZEPHYR_ENTRY_PORT: u16 = 17855;
const WINDOW_SECS: u64 = 20;

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
#[ignore = "phase-279 W1/W3 manual measurement — 22 s window, needs the zephyr \
            west realtime fixture; run with `--ignored`"]
fn w1_measure_zephyr_tx_throughput() {
    let entry = build_zephyr_workspace_rust_realtime_entry()
        .unwrap_or_else(|e| nros_tests::skip!("zephyr realtime workspace entry not built: {e}"));
    let router = ZenohRouter::start_on("127.0.0.1", REALTIME_ZEPHYR_ENTRY_PORT)
        .unwrap_or_else(|e| nros_tests::skip!("zenohd failed: {e}"));
    let locator = router.locator();

    let mut ctrl = spawn_listener("/ctrl", &locator);
    let mut telem = spawn_listener("/telem", &locator);

    let mut zephyr = ZephyrProcess::start(&entry, ZephyrPlatform::NativeSim)
        .unwrap_or_else(|e| panic!("boot zephyr native_sim: {e}"));

    // Fixed window: drain ctrl for the full window (its ~2 k msgs fit the OS pipe
    // buffer), then drain telem's buffered output (accumulated unread during the
    // ctrl drain; ~200 msgs, also fits). `wait_for_all_output` reads for the whole
    // timeout then kills the process, so counts cover ~WINDOW_SECS for both tiers.
    let ctrl_out = ctrl
        .wait_for_all_output(Duration::from_secs(WINDOW_SECS))
        .unwrap_or_default();
    let telem_out = telem
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();

    zephyr.kill();

    let ctrl_n =
        nros_tests::count_pattern(&ctrl_out, nros_tests::output::INT32_LISTENER_LOG_PREFIX);
    let telem_n =
        nros_tests::count_pattern(&telem_out, nros_tests::output::INT32_LISTENER_LOG_PREFIX);
    let w = WINDOW_SECS as f64;
    eprintln!("\n===== phase-279 W1 measurement (window {WINDOW_SECS}s) =====");
    eprintln!(
        "ctrl  (100 Hz tier): {ctrl_n} msgs = {:.1} msg/s (ideal 100)",
        ctrl_n as f64 / w
    );
    eprintln!(
        "telem ( 10 Hz tier): {telem_n} msgs = {:.1} msg/s (ideal 10)",
        telem_n as f64 / w
    );
    eprintln!(
        "TOTAL tx: {} msgs = {:.1} msg/s\n",
        ctrl_n + telem_n,
        (ctrl_n + telem_n) as f64 / w
    );
    assert!(
        ctrl_n + telem_n > 0,
        "no messages received — harness/build broken, not a throughput number"
    );
}
