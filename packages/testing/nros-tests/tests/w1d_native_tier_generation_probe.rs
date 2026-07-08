//! phase-282 W1.d follow-up (#145) — MEASUREMENT SCRATCH, not a CI gate
//! (`#[ignore]`).
//!
//! W1 landed the tx split-lock, yet the native 100 Hz `/ctrl` tier still tops
//! out at ~40 delivered msg/s in every config. W1.d's hypothesis: with the tx
//! path unblocked the residual cap has MOVED OFF tx onto message *generation*
//! (the executor timer not firing 100×/s under native_sim jitter), not tx drops.
//!
//! This probe discriminates the two cheaply, with no talker instrumentation: the
//! ctrl node publishes a MONOTONIC counter as the payload, so the delivered Int32
//! *values* encode the published sequence.
//!   - `max_value / window`  ≈ PUBLISHED rate (the highest counter that was ever
//!     minted and reached the sink).
//!   - `count / window`      ≈ DELIVERED rate.
//! Verdict:
//!   - `max ≈ count` (both ~40/s)  → GENERATION-limited: the timer only fired
//!     ~40×/s; tx is not the bottleneck (W1.d confirmed).
//!   - `max ≫ count` (max ~100/s, count ~40/s) → TX-limited: the timer fired
//!     ~100×/s but ~60% were dropped on the tx path.
//!
//! Run: `cargo nextest run -p nros-tests --test w1d_native_tier_generation_probe --ignored --no-capture`

use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, build_int32_sink, build_native_workspace_rust_realtime_entry,
    require_zenohd, zenohd_unique,
};
use rstest::rstest;
use std::{process::Command, time::Duration};

const WINDOW_SECS: u64 = 15;

/// Highest `Received: <n>` value in the sink output (≈ total published), and the
/// number of `Received:` lines (delivered count).
fn max_and_count(out: &str) -> (i64, usize) {
    let mut max = 0i64;
    let mut count = 0usize;
    for line in out.lines() {
        if let Some(rest) = line.split("Received:").nth(1) {
            if let Ok(v) = rest.trim().split_whitespace().next().unwrap_or("").parse::<i64>() {
                max = max.max(v);
                count += 1;
            }
        }
    }
    (max, count)
}

#[rstest]
#[ignore = "phase-282 W1.d manual measurement — 15 s window, needs the native \
            realtime workspace entry; run with `--ignored`"]
fn w1d_ctrl_tier_generation_vs_tx(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    let entry = build_native_workspace_rust_realtime_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("realtime workspace entry fixture not built: {e}"));
    let locator = zenohd_unique.locator();

    // /ctrl sink (100 Hz tier). Deep enough spin window to run the full measurement.
    let mut ctrl = {
        let listener = build_int32_sink()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|e| nros_tests::skip!("int32-sink fixture not built: {e}"));
        let mut cmd = Command::new(listener);
        cmd.env("RUST_LOG", "info")
            .env("NROS_LOCATOR", &locator)
            .env("NROS_SESSION_MODE", "client")
            .env("NROS_SUB_TOPIC", "/ctrl");
        let mut p = ManagedProcess::spawn_command(cmd, "/ctrl")
            .unwrap_or_else(|e| panic!("spawn /ctrl sink: {e}"));
        p.wait_for_output_pattern("Listener", Duration::from_secs(8))
            .unwrap_or_else(|_| panic!("/ctrl listener did not become ready"));
        p
    };

    // Boot the entry for the full window (ctrl tier = 10 ms period → ideal 100/s).
    let mut proc = {
        let mut cmd = Command::new(entry);
        cmd.env("RUST_LOG", "info")
            .env("NROS_LOCATOR", &locator)
            .env("NROS_SESSION_MODE", "client")
            .env("NROS_ENTRY_SPIN_MS", (WINDOW_SECS * 1000).to_string())
            .env("NROS_ENTRY_SPIN_STEP_MS", "5");
        ManagedProcess::spawn_command(cmd, "realtime").expect("spawn realtime entry")
    };

    // Collect the sink's output for the full window (kills the sink at the end),
    // then stop the entry.
    let out = ctrl
        .wait_for_all_output(Duration::from_secs(WINDOW_SECS))
        .unwrap_or_default();

    proc.kill();

    let (max, count) = max_and_count(&out);
    let pub_rate = max as f64 / WINDOW_SECS as f64;
    let del_rate = count as f64 / WINDOW_SECS as f64;
    eprintln!(
        "W1.d probe [{WINDOW_SECS}s]: /ctrl published≈{max} ({pub_rate:.1}/s), \
         delivered={count} ({del_rate:.1}/s), delivered/published={:.0}%",
        if max > 0 { count as f64 / max as f64 * 100.0 } else { 0.0 }
    );
    let verdict = if max > 0 && (count as f64) >= 0.8 * (max as f64) {
        "GENERATION-limited (max≈count: the timer under-fires; tx is NOT the cap)"
    } else if max > 0 {
        "TX-limited (max≫count: timer fires fast, tx drops)"
    } else {
        "INCONCLUSIVE (no /ctrl values received)"
    };
    eprintln!("W1.d verdict: {verdict}");

    // Precondition, not the finding: SOMETHING must have been delivered, else the
    // run is broken (fail loud per CLAUDE.md — never silently PASS on 0).
    assert!(
        count > 0,
        "no /ctrl messages delivered — realtime entry or router broken, probe inconclusive"
    );
}
