//! `/rosout` live-peer fixture — phase-467 Q4, ledger row
//! `c:logging_rosout_enabled`.
//!
//! Logs through its node's logger and spins. Nothing else: since phase-556
//! W13 the executor creates the node's `/rosout` writer and publishes the log
//! queue from every spin, so the question "does a STOCK `ros2 topic echo
//! /rosout` see a nano-ros node's logs?" is asked of the runtime's writer.
//! Consumed by `packages/testing/nros-tests/tests/rosout_interop.rs`
//! (interop cell `native-logging-rust-zenoh-n2r`).
//!
//! Env in:
//! * `NROS_LOCATOR`          — the zenoh router to connect to. Required.
//! * `ROSOUT_PROBE_RECORDS`  — how many records to raise (default 60).
//! * `ROSOUT_PROBE_PERIOD_MS`— gap between records (default 250).
//!
//! Markers out (stdout, flushed — a test greps these):
//! * `ROSOUT_PROBE_READY`      the node exists
//! * `ROSOUT_PROBE_PUMPED n`   `n` records have reached a writer in total
//! * `ROSOUT_PROBE_DONE ...`   the run's accounting

use std::io::Write as _;

use nros::prelude::*;
use nros_log::{log_info, log_warn};

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn say(line: &str) {
    println!("{line}");
    let _ = std::io::stdout().flush();
}

fn main() {
    env_logger::init();
    nros_board_linux::register_linked_rmw();
    nros_log::init(nros_platform_cffi::log::default_sinks());

    let records = env_usize("ROSOUT_PROBE_RECORDS", 60);
    let period_ms = env_usize("ROSOUT_PROBE_PERIOD_MS", 250) as u64;

    let ctx = nros::init().expect("nros::init failed");
    let cfg = ctx.config("rosout_talker");
    let mut executor: Executor = Executor::open(&cfg).expect("Failed to open session");

    // RFC-0102 D4 — records come from the NODE's logger, as a real node's do.
    // A free logger is published by no ROS release: rcl publishes only through
    // a publisher correlated with a node's logger, and the `/rosout` queue is
    // scoped to match.
    let probe = executor
        .create_node("rosout_talker")
        .expect("Failed to create node")
        .logger();
    say("ROSOUT_PROBE_READY");

    for i in 0..records {
        log_info!(probe, "nros rosout probe record {i}");
        // Every other record, not every tenth. The test's wait is a CONDITION
        // on the count of `record` lines, so it returns as soon as a handful
        // land; a WARN at `i % 10 == 9` was never inside that window and the
        // severity assertion failed on a run that had in fact delivered
        // everything it was asked for. The cadence and the wait are one
        // decision.
        if i % 2 == 1 {
            log_warn!(probe, "nros rosout probe warning at {i}");
        }
        let _ = executor.spin_once(core::time::Duration::from_millis(period_ms));
        say(&format!("ROSOUT_PROBE_PUMPED {}", nros::rosout::published()));
    }

    // Report what the run cost, so a reader of the log knows whether the
    // queue depth was enough rather than inferring it from a short echo.
    say(&format!(
        "ROSOUT_PROBE_DONE pumped={} dropped={} suppressed={}",
        nros::rosout::published(),
        nros::rosout::dropped(),
        nros::rosout::suppressed()
    ));
}
