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
//!   - `(max_value + 1) / window`  ≈ PUBLISHED rate (the counter is 0-indexed, so
//!     the highest value seen means `max + 1` were minted).
//!   - `count / window`            ≈ DELIVERED rate.
//!
//! Verdict:
//!
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
        if let Some(rest) = line
            .split(nros_tests::output::INT32_LISTENER_LOG_PREFIX)
            .nth(1)
            && let Ok(v) = rest.split_whitespace().next().unwrap_or("").parse::<i64>()
        {
            max = max.max(v);
            count += 1;
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

    // #162 — GATE the measurement on a first delivery before opening the window.
    // The sink's "Listener" banner precedes zenoh route establishment, so an entry
    // can boot + publish into the gossip gap and never match (~1/11 runs → a 0
    // window). Boot the entry, wait (bounded) for the sink's FIRST `Received:`; on
    // the startup race retry the boot ONCE, then fail loud. Each entry spins long
    // enough to cover the first-delivery wait + the measurement window.
    let spin_ms = ((WINDOW_SECS + 14) * 1000).to_string();
    let boot_entry = || {
        let mut cmd = Command::new(&entry);
        cmd.env("RUST_LOG", "info")
            .env("NROS_LOCATOR", &locator)
            .env("NROS_SESSION_MODE", "client")
            .env("NROS_ENTRY_SPIN_MS", &spin_ms)
            .env("NROS_ENTRY_SPIN_STEP_MS", "5");
        ManagedProcess::spawn_command(cmd, "realtime").expect("spawn realtime entry")
    };

    let mut proc = boot_entry();
    let mut first = match ctrl.wait_for_output_count(
        nros_tests::output::INT32_LISTENER_LOG_PREFIX,
        1,
        Duration::from_secs(12),
    ) {
        Ok(out) => out,
        Err(_) => {
            // Startup race: entry published into the gossip gap. Retry ONE boot.
            eprintln!("W1.d probe: no first delivery in 12 s — startup race, retrying boot once");
            proc.kill();
            proc = boot_entry();
            ctrl.wait_for_output_count(
                nros_tests::output::INT32_LISTENER_LOG_PREFIX,
                1,
                Duration::from_secs(12),
            )
            .unwrap_or_else(|_| {
                proc.kill();
                ctrl.kill();
                panic!(
                    "no /ctrl delivery after two boots — realtime entry or router broken \
                         (not a transient startup race)"
                )
            })
        }
    };

    // First delivery seen → open the measurement window and drain the rest.
    let rest = ctrl
        .wait_for_all_output(Duration::from_secs(WINDOW_SECS))
        .unwrap_or_default();
    proc.kill();
    first.push_str(&rest);
    let out = first;

    let (max, count) = max_and_count(&out);
    // #162 — the ctrl counter starts at 0, so the published count is `max + 1`.
    let published = max + 1;
    let pub_rate = published as f64 / WINDOW_SECS as f64;
    let del_rate = count as f64 / WINDOW_SECS as f64;
    eprintln!(
        "W1.d probe [{WINDOW_SECS}s]: /ctrl published≈{published} ({pub_rate:.1}/s), \
         delivered={count} ({del_rate:.1}/s), delivered/published={:.0}%",
        count as f64 / published as f64 * 100.0
    );
    // Gated above → count is always ≥1 here; the verdict is the finding. The ctrl
    // tier is 10 ms → 100/s ideal. (Post-#148: on cleanly-built fixtures this reads
    // IDEAL — 100/s published, ~100 % delivered — refuting the original
    // "generation-limited" hypothesis; the 80 % once seen was a stale-object build.)
    const IDEAL_RATE: f64 = 100.0; // 10 ms period
    let verdict = if pub_rate < 0.9 * IDEAL_RATE {
        "GENERATION-limited (published ≪ 100/s: the timer under-fires; not a tx cap)"
    } else if (count as f64) < 0.8 * (published as f64) {
        "TX-limited (published at line rate but a large fraction dropped)"
    } else {
        "IDEAL (published ~100/s AND ~100% delivered — no generation or tx cap)"
    };
    eprintln!("W1.d verdict: {verdict}");

    assert!(
        count > 0,
        "no /ctrl messages delivered after the first-delivery gate — impossible unless \
         the sink died mid-window"
    );
}
