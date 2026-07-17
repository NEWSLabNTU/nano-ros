//! RFC-0052 / phase-296 W3b.4 — on-target contract monitors.
//!
//! The baked shape mirrors Phase 211.H's `qos_overrides`: codegen emits a
//! `&'static [MonitorSpec]` table (plus one `static PubMonitorCell` per
//! contracted publisher) from the SystemModel's contract layer; the entry
//! installs it on the executor before entity creation. An uncontracted
//! image bakes an empty table — every path below dead-code-eliminates.
//!
//! Publish counting is an atomic bump on the publisher handle (no clock,
//! no lock on the hot path); the rate check runs on spin ticks over a
//! ~[`RATE_CHECK_INTERVAL_US`] window and pushes violations into a small
//! ring the entry glue drains into the `nros-diagnostics` reporter.

use core::sync::atomic::{AtomicU32, Ordering};

/// One contracted publisher's counter. Baked as a `static` by codegen (or
/// declared by the fixture); the publisher handle bumps it on every
/// publish, the executor reads deltas on spin ticks.
#[derive(Debug, Default)]
pub struct PubMonitorCell {
    pub count: AtomicU32,
}

impl PubMonitorCell {
    pub const fn new() -> Self {
        Self {
            count: AtomicU32::new(0),
        }
    }
}

/// One monitored publisher endpoint.
#[derive(Debug, Clone, Copy)]
pub struct MonitorSpec {
    /// Topic name EXACTLY as the node passes it to `create_publisher`
    /// (the SystemModel's wiring carries the same resolved name).
    pub topic: &'static str,
    /// Endpoint ref for violation reports (`<node FQN>/<endpoint>` — the
    /// SystemModel contract key).
    pub fqn: &'static str,
    /// Declared publisher guarantee, in milli-Hz (fixed point: Hz × 1000).
    /// 0 = no rate contract on this endpoint.
    pub min_rate_hz_milli: u32,
    /// The endpoint's counter cell.
    pub cell: &'static PubMonitorCell,
}

/// Rate-check window (µs). Matches play_launch's ~5 s time-based trigger
/// so both runtimes converge on comparable cadence.
pub const RATE_CHECK_INTERVAL_US: u64 = 5_000_000;

/// Max monitored endpoints per executor (const table, no_std).
pub const MAX_MONITORS: usize = 8;
/// Violation ring depth.
pub const MAX_VIOLATIONS: usize = 8;

/// A detected contract violation, in the play_launch rule-id vocabulary.
#[derive(Debug, Clone)]
pub struct Violation {
    /// `"rate-hierarchy-runtime"` (this module) — age/latency rules land
    /// with W3b.5.
    pub rule: &'static str,
    /// Violating endpoint ref (from [`MonitorSpec::fqn`]).
    pub fqn: &'static str,
    /// Measured value, milli-Hz.
    pub measured_milli_hz: u32,
    /// Declared bound, milli-Hz.
    pub declared_milli_hz: u32,
}

/// Per-spec accounting state (parallel to the spec table).
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct MonitorState {
    /// Window opened (a plain bool, not a 0-sentinel on the timestamp —
    /// `now_us == 0` is a legitimate first sample on freshly-started
    /// monotonic clocks).
    pub(crate) opened: bool,
    pub(crate) window_start_us: u64,
    pub(crate) count_at_window_start: u32,
    /// Suppress duplicate reports: only re-report after a clean window.
    pub(crate) violated_last_window: bool,
}

/// Pure rate check over one window boundary. Returns `Some(violation)`
/// when the window elapsed AND the measured rate is below the declared
/// minimum (and we didn't already report last window).
///
/// Extracted from the executor so the math is unit-testable without a
/// session: publish counting is injected via the cell, time via `now_us`.
pub(crate) fn check_rate(
    spec: &MonitorSpec,
    state: &mut MonitorState,
    now_us: u64,
) -> Option<Violation> {
    if spec.min_rate_hz_milli == 0 {
        return None;
    }
    let count = spec.cell.count.load(Ordering::Relaxed);
    if !state.opened {
        // First observation: open the window, no verdict yet.
        state.opened = true;
        state.window_start_us = now_us;
        state.count_at_window_start = count;
        return None;
    }
    let window_us = now_us.saturating_sub(state.window_start_us);
    if window_us < RATE_CHECK_INTERVAL_US {
        return None;
    }
    let published = count.wrapping_sub(state.count_at_window_start) as u64;
    // milli-Hz = published * 1e3 / window_s = published * 1e9 / window_us
    let measured_milli_hz =
        (published.saturating_mul(1_000_000_000) / window_us.max(1)).min(u32::MAX as u64) as u32;

    // Roll the window.
    state.window_start_us = now_us;
    state.count_at_window_start = count;

    if measured_milli_hz < spec.min_rate_hz_milli {
        if state.violated_last_window {
            return None; // still violated — already reported
        }
        state.violated_last_window = true;
        Some(Violation {
            rule: "rate-hierarchy-runtime",
            fqn: spec.fqn,
            measured_milli_hz,
            declared_milli_hz: spec.min_rate_hz_milli,
        })
    } else {
        state.violated_last_window = false;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static CELL: PubMonitorCell = PubMonitorCell::new();

    fn spec(min_milli: u32) -> MonitorSpec {
        MonitorSpec {
            topic: "/chatter",
            fqn: "/demo/talker/chatter",
            min_rate_hz_milli: min_milli,
            cell: &CELL,
        }
    }

    #[test]
    fn slow_publisher_fires_once_until_recovery() {
        CELL.count.store(0, Ordering::Relaxed);
        let s = spec(100_000); // 100 Hz declared
        let mut st = MonitorState::default();

        // t=0: opens window.
        assert!(check_rate(&s, &mut st, 0).is_none());
        // 5 publishes in 5 s = 1 Hz — violation.
        CELL.count.store(5, Ordering::Relaxed);
        let v = check_rate(&s, &mut st, RATE_CHECK_INTERVAL_US).expect("fires");
        assert_eq!(v.rule, "rate-hierarchy-runtime");
        assert_eq!(v.fqn, "/demo/talker/chatter");
        assert_eq!(v.measured_milli_hz, 1_000);
        assert_eq!(v.declared_milli_hz, 100_000);
        // Still slow next window — suppressed (no re-report spam).
        CELL.count.store(10, Ordering::Relaxed);
        assert!(check_rate(&s, &mut st, 2 * RATE_CHECK_INTERVAL_US).is_none());
        // Recovers (500 publishes in 5 s = 100 Hz) — clean window resets.
        CELL.count.store(510, Ordering::Relaxed);
        assert!(check_rate(&s, &mut st, 3 * RATE_CHECK_INTERVAL_US).is_none());
        // Degrades again — fires again.
        CELL.count.store(511, Ordering::Relaxed);
        assert!(check_rate(&s, &mut st, 4 * RATE_CHECK_INTERVAL_US).is_some());
    }

    #[test]
    fn compliant_and_uncontracted_stay_silent() {
        static C2: PubMonitorCell = PubMonitorCell::new();
        let s = MonitorSpec {
            topic: "/t",
            fqn: "/n/t",
            min_rate_hz_milli: 500, // 0.5 Hz
            cell: &C2,
        };
        let mut st = MonitorState::default();
        assert!(check_rate(&s, &mut st, 0).is_none());
        C2.count.store(5, Ordering::Relaxed); // 1 Hz measured ≥ 0.5 Hz declared
        assert!(check_rate(&s, &mut st, RATE_CHECK_INTERVAL_US).is_none());

        // min_rate 0 = uncontracted: never fires, no state.
        let s0 = MonitorSpec {
            topic: "/t",
            fqn: "/n/t",
            min_rate_hz_milli: 0,
            cell: &C2,
        };
        let mut st0 = MonitorState::default();
        assert!(check_rate(&s0, &mut st0, 10 * RATE_CHECK_INTERVAL_US).is_none());
    }
}
