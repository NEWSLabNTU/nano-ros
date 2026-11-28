// Issue 3010.

//! The FreeRTOS POSIX port's tick keeps host time.
//!
//! On the freertos-posix board `nros_platform_clock_ns` is the kernel tick
//! count, so a tick that runs slow makes every timer, rate and timeout in an
//! image run slow by the same factor. The port's tick is a host timer thread
//! signalling the scheduler thread, and a signal that falls due while the tick
//! is masked coalesces with the next one; the upstream port advanced the tick
//! once per signal, so it lost time whenever the scheduler thread could not
//! take a signal promptly. Measured under host load: 9.3 % slow at load
//! average ~5, 12.6 % at ~37, which an ASI control loop saw as a 33.33 Hz
//! rate running at ~29 Hz. The patched port delivers every tick that fell due
//! against `CLOCK_MONOTONIC`.
//!
//! # Why the load is injected, not real
//!
//! Host load is not reproducible, and a test that made it would reach every
//! other process on the machine. The probe (`fixtures/freertos-posix-tick-drift`)
//! makes its own, deterministically: ten times a second it masks the tick for
//! 30 ms of host time, then sleeps 70 ms. One core, five seconds.
//!
//! Negative control, measured on the same probe built against the upstream
//! port at the old pin: 36–42 % slow. Patched: within ±0.02 %, solo and under
//! a concurrent build.
//!
//! Run with: `cargo nextest run -p nros-tests --test freertos_posix_tick`

use nros_tests::{
    fixtures::{ManagedProcess, RequireFixture, require_compile_check_bin},
    output::TICK_DRIFT_RESULT_PREFIX,
};
use std::{process::Command, time::Duration};

/// Seconds of host time the probe runs (about 50 masked periods). It loops
/// by host time, so `host_s` below this means it exited early.
const PROBE_SECONDS: &str = "5";
const WINDOW: Duration = Duration::from_secs(60);

/// The bound on |drift|, in percent, in BOTH directions: slow is the bug, and
/// a tick running fast would be a catch-up that over-delivers. 0.5 % is ~70x
/// below the unpatched port's loss on this probe and ~25x above the patched
/// port's worst measured value.
const DRIFT_BOUND_PCT: f64 = 0.5;

fn field(line: &str, key: &str) -> f64 {
    line.split_whitespace()
        .find_map(|kv| kv.strip_prefix(key)?.strip_prefix('='))
        .unwrap_or_else(|| panic!("probe line has no `{key}=` field: {line}"))
        .parse()
        .unwrap_or_else(|e| panic!("probe `{key}` is not a number ({e}): {line}"))
}

#[test]
fn freertos_posix_tick_keeps_host_time_while_masked() {
    let probe = require_compile_check_bin("freertos_posix_tick_drift", "tick-drift")
        .require("the FreeRTOS POSIX tick-drift probe");

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
    // On the record for a passing run too: the margin is the evidence.
    eprintln!("{line}");

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
         due while the tick was masked were not delivered.\n{out}"
    );
}
