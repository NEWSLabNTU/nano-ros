// Issue 3017.

//! The threadx-linux port's tick keeps host time.
//!
//! On the threadx-linux board `nros_platform_clock_ns` is the kernel tick
//! (`tx_time_get`), so a tick that runs slow makes every timer, rate and
//! timeout in an image run slow by the same factor.
//!
//! The tick comes from the Linux port's simulated timer interrupt, a host
//! thread. It used to wait "now + one period" and deliver one tick per
//! wakeup, so every wakeup's latency was lost time for good. The port now
//! counts the ticks due from host `CLOCK_MONOTONIC` and delivers every one it
//! owes. Measured on the probe below: the old port loses ~22.6 %, the fixed
//! one stays within ±0.002 %.
//!
//! # Why the load is injected, not real
//!
//! A loaded host shows the defect too, but only by as much as the load
//! happens to delay the timer thread: 0.5–0.7 % on a 32-core host, depending
//! on what else runs. A test that loads the host would measure its runner as
//! much as the port, and would slow everything sharing that runner. So the
//! probe (`fixtures/threadx-linux-tick-drift/main.c`) makes the delay itself:
//! it masks interrupts for 30 ms of host time about ten times a second.
//! `TX_DISABLE` holds the port's mutex, so the timer thread blocks for the
//! whole mask, and a port that does not catch up loses three ticks each time.
//! The result is the same on an idle host and a busy one.
//!
//! Run with: `cargo nextest run -p nros-tests --test threadx_linux_tick`

use nros_tests::{
    fixtures::{RequireFixture, require_compile_check_bin},
    output::TICK_DRIFT_RESULT_PREFIX,
    process::ManagedProcess,
};
use std::{process::Command, time::Duration};

/// Seconds of host time the probe runs: about 50 masked periods.
const PROBE_SECONDS: &str = "5";
const WINDOW: Duration = Duration::from_secs(60);

/// The bound on |drift|, in percent, in BOTH directions: slow is the bug, and
/// a tick running fast would be a catch-up that over-delivers. 0.5 % is ~45x
/// below the old port's loss on this probe and ~250x above the fixed port's
/// worst measured value.
const DRIFT_BOUND_PCT: f64 = 0.5;

fn field(line: &str, key: &str) -> f64 {
    line.split_whitespace()
        .find_map(|kv| kv.strip_prefix(key)?.strip_prefix('='))
        .unwrap_or_else(|| panic!("probe line has no `{key}=` field: {line}"))
        .parse()
        .unwrap_or_else(|e| panic!("probe `{key}` is not a number ({e}): {line}"))
}

#[test]
fn threadx_linux_tick_keeps_host_time_while_masked() {
    let probe = require_compile_check_bin("threadx_linux_tick_drift", "tick-drift")
        .require("the threadx-linux tick-drift probe");

    let mut cmd = Command::new(&probe);
    cmd.arg(PROBE_SECONDS);
    let mut proc = ManagedProcess::spawn_image(cmd, "tick-drift")
        .unwrap_or_else(|e| panic!("spawn the tick-drift probe at {}: {e}", probe.display()));
    let out = proc.wait_for_all_output(WINDOW).unwrap_or_else(|e| {
        panic!("the tick-drift probe produced no output within {WINDOW:?}: {e}")
    });

    let line = out
        .lines()
        .find(|l| l.starts_with(TICK_DRIFT_RESULT_PREFIX))
        .unwrap_or_else(|| panic!("the probe printed no result line:\n{out}"));
    let host_s = field(line, "host_s");
    let tick_s = field(line, "tick_s");
    let drift = field(line, "drift_pct");

    // The probe ran the time it was asked for; a probe that exited early
    // would make any drift figure meaningless.
    assert!(
        host_s >= 5.0,
        "the probe ran {host_s} s of host time, under the {PROBE_SECONDS} s asked:\n{out}"
    );
    assert!(
        drift.abs() < DRIFT_BOUND_PCT,
        "the tick drifted {drift} % from host time ({tick_s} s of tick over {host_s} s \
         of host time; bound ±{DRIFT_BOUND_PCT} %). Positive is slow: ticks that fell \
         due while interrupts were masked were not delivered.\n{out}"
    );
}
