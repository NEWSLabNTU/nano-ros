//! RFC-0052 / phase-296 W3b.4/.5 — on-target contract monitors.
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
//!
//! W3b.5 adds three more rules on the same drain:
//! - `max-age-runtime` — subscriber take-age (`epoch_now - header.stamp`
//!   peeked from the raw CDR buffer at [`RosMessage::STAMP_OFFSET`],
//!   recorded into a [`SubMonitorCell`] on the take path).
//! - `max-latency-runtime` — node-path (take → publish) latency: the
//!   dispatch elapsed time is attributed to every monitored publisher
//!   whose counter advanced during that dispatch (an upper bound on
//!   take → publish, measured on the executor's monotonic clock).
//! - `deadline-miss-runtime` — a dispatched callback ran past its bound
//!   SchedContext's `deadline_us`; what ELSE happens is the tier's
//!   [`DeadlineAction`](super::sched_context::DeadlineAction).

use core::sync::atomic::Ordering;
// portable-atomic: RMW ops (fetch_add/fetch_max/swap) exist even on
// riscv32imc / Cortex-M0+ that lack native CAS (same choice as
// `SporadicState` / `AtomicSporadicState` in sched_context.rs).
use portable_atomic::AtomicU32;

/// One contracted publisher's counters. Baked as a `static` by codegen
/// (or declared by the fixture); the publisher handle bumps `count` on
/// every publish, the executor reads deltas on spin ticks.
#[derive(Debug, Default)]
pub struct PubMonitorCell {
    pub count: AtomicU32,
    /// W3b.5 — max observed take→publish latency (µs) in the current
    /// check window. Written by the dispatch loop (fetch_max), drained
    /// (swap 0) by the latency check.
    pub max_latency_us: AtomicU32,
}

impl PubMonitorCell {
    pub const fn new() -> Self {
        Self {
            count: AtomicU32::new(0),
            max_latency_us: AtomicU32::new(0),
        }
    }
}

/// One contracted subscriber's take-age accumulator (W3b.5). The take
/// path records `epoch_now - header.stamp` per message (fetch_max); the
/// age check drains it (swap 0) per window.
#[derive(Debug, Default)]
pub struct SubMonitorCell {
    /// Max observed take-age (ms) in the current check window.
    pub max_age_ms: AtomicU32,
}

impl SubMonitorCell {
    pub const fn new() -> Self {
        Self {
            max_age_ms: AtomicU32::new(0),
        }
    }

    /// Take-path hook: record one message's age. `stamp_us` is the
    /// peeked `header.stamp` as µs since the UNIX epoch, `epoch_now_us`
    /// the receive-side wall clock. A stamp from the future clamps to 0.
    pub fn observe(&self, stamp_us: u64, epoch_now_us: u64) {
        let age_ms = (epoch_now_us.saturating_sub(stamp_us) / 1_000).min(u32::MAX as u64) as u32;
        self.max_age_ms.fetch_max(age_ms, Ordering::Relaxed);
    }
}

/// Peek `Time { i32 sec; u32 nanosec }` little-endian at `offset` in a
/// raw CDR receive buffer (encapsulation header included) and return µs
/// since the UNIX epoch. `None` when the buffer is too short or the
/// stamp is pre-epoch/zero (unstamped messages never fire age monitors).
pub fn peek_stamp_us(raw: &[u8], offset: usize) -> Option<u64> {
    let sec_b = raw.get(offset..offset + 4)?;
    let nsec_b = raw.get(offset + 4..offset + 8)?;
    let sec = i32::from_le_bytes([sec_b[0], sec_b[1], sec_b[2], sec_b[3]]);
    let nsec = u32::from_le_bytes([nsec_b[0], nsec_b[1], nsec_b[2], nsec_b[3]]);
    if sec <= 0 {
        return None;
    }
    Some(sec as u64 * 1_000_000 + nsec as u64 / 1_000)
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
    /// W3b.5 — node-path budget (ms) for paths whose OUTPUT is this
    /// endpoint (`contracts.node_paths[..].max_latency_ms`). 0 = no
    /// latency contract.
    pub max_latency_ms: u32,
    /// The endpoint's counter cell.
    pub cell: &'static PubMonitorCell,
}

/// One monitored subscriber endpoint (W3b.5 age contracts). Separate
/// table from [`MonitorSpec`] — sub contracts key different endpoints
/// and need no publish counter.
#[derive(Debug, Clone, Copy)]
pub struct AgeMonitorSpec {
    /// Topic name EXACTLY as the node passes it to `create_subscription`.
    pub topic: &'static str,
    /// Endpoint ref for violation reports (the SystemModel contract key).
    pub fqn: &'static str,
    /// Declared max take-age (ms). 0 = no age contract.
    pub max_age_ms: u32,
    /// The endpoint's age accumulator.
    pub cell: &'static SubMonitorCell,
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
    /// `"rate-hierarchy-runtime"` | `"max-age-runtime"` |
    /// `"max-latency-runtime"` | `"deadline-miss-runtime"`.
    pub rule: &'static str,
    /// Violating endpoint ref (from the spec's `fqn`; the SC name for
    /// deadline misses).
    pub fqn: &'static str,
    /// Measured value. Unit is per-rule: milli-Hz for the rate rule, ms
    /// for age/latency, µs for deadline misses.
    pub measured: u32,
    /// Declared bound, same unit as `measured`.
    pub declared: u32,
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
    /// W3b.5 — separate dedup for the latency rule on the same spec row.
    pub(crate) latency_violated_last_window: bool,
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
            measured: measured_milli_hz,
            declared: spec.min_rate_hz_milli,
        })
    } else {
        state.violated_last_window = false;
        None
    }
}

/// Pure latency check: drains the spec cell's window-max take→publish
/// latency and fires when it exceeds the declared node-path budget.
/// Same report-once-until-recovery semantics as the rate rule; runs on
/// every monitor tick (the cell accumulates between ticks, so no window
/// bookkeeping is needed — draining IS the window roll).
pub(crate) fn check_latency(spec: &MonitorSpec, state: &mut MonitorState) -> Option<Violation> {
    if spec.max_latency_ms == 0 {
        return None;
    }
    let max_us = spec.cell.max_latency_us.swap(0, Ordering::Relaxed);
    let max_ms = max_us / 1_000;
    if max_ms > spec.max_latency_ms {
        if state.latency_violated_last_window {
            return None;
        }
        state.latency_violated_last_window = true;
        Some(Violation {
            rule: "max-latency-runtime",
            fqn: spec.fqn,
            measured: max_ms,
            declared: spec.max_latency_ms,
        })
    } else {
        // A quiet window (no dispatch attributed) also counts as clean —
        // recovery resets the dedup like the rate rule's clean window.
        state.latency_violated_last_window = false;
        None
    }
}

/// Per-age-spec dedup state.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct AgeState {
    pub(crate) violated_last_window: bool,
}

/// Pure age check: drains the sub cell's window-max take-age and fires
/// when it exceeds the declared bound. Report-once-until-recovery.
pub(crate) fn check_age(spec: &AgeMonitorSpec, state: &mut AgeState) -> Option<Violation> {
    if spec.max_age_ms == 0 {
        return None;
    }
    let max_ms = spec.cell.max_age_ms.swap(0, Ordering::Relaxed);
    if max_ms > spec.max_age_ms {
        if state.violated_last_window {
            return None;
        }
        state.violated_last_window = true;
        Some(Violation {
            rule: "max-age-runtime",
            fqn: spec.fqn,
            measured: max_ms,
            declared: spec.max_age_ms,
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
            max_latency_ms: 0,
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
        assert_eq!(v.measured, 1_000);
        assert_eq!(v.declared, 100_000);
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
            max_latency_ms: 0,
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
            max_latency_ms: 0,
            cell: &C2,
        };
        let mut st0 = MonitorState::default();
        assert!(check_rate(&s0, &mut st0, 10 * RATE_CHECK_INTERVAL_US).is_none());
    }

    #[test]
    fn stale_take_fires_age_once_until_recovery() {
        static SC: SubMonitorCell = SubMonitorCell::new();
        let s = AgeMonitorSpec {
            topic: "/scan",
            fqn: "/perc/detector/scan",
            max_age_ms: 100,
            cell: &SC,
        };
        let mut st = AgeState::default();

        // Fresh message: stamped 5 ms ago — silent.
        SC.observe(1_000_000_000, 1_000_005_000);
        assert!(check_age(&s, &mut st).is_none());
        // Stale: 250 ms old — fires with the measured age.
        SC.observe(1_000_000_000, 1_000_250_000);
        let v = check_age(&s, &mut st).expect("fires");
        assert_eq!(v.rule, "max-age-runtime");
        assert_eq!(v.fqn, "/perc/detector/scan");
        assert_eq!(v.measured, 250);
        assert_eq!(v.declared, 100);
        // Still stale next window — suppressed.
        SC.observe(1_000_000_000, 1_000_300_000);
        assert!(check_age(&s, &mut st).is_none());
        // Recovers — clean window resets; stale again refires.
        SC.observe(1_000_000_000, 1_000_010_000);
        assert!(check_age(&s, &mut st).is_none());
        SC.observe(1_000_000_000, 1_000_999_000);
        assert!(check_age(&s, &mut st).is_some());
    }

    #[test]
    fn peek_stamp_reads_le_time_and_rejects_unstamped() {
        // Encapsulation header (4B) + sec=100 nsec=5000 at offset 4.
        let mut raw = [0u8; 12];
        raw[4..8].copy_from_slice(&100i32.to_le_bytes());
        raw[8..12].copy_from_slice(&5_000u32.to_le_bytes());
        assert_eq!(peek_stamp_us(&raw, 4), Some(100_000_005));
        // Zero / negative sec = unstamped: no age sample.
        assert_eq!(peek_stamp_us(&[0u8; 12], 4), None);
        // Short buffer: no panic, no sample.
        assert_eq!(peek_stamp_us(&raw[..8], 4), None);
    }

    #[test]
    fn slow_path_fires_latency_once_until_recovery() {
        static C3: PubMonitorCell = PubMonitorCell::new();
        let s = MonitorSpec {
            topic: "/cmd",
            fqn: "/ctrl/control/cmd",
            min_rate_hz_milli: 0,
            max_latency_ms: 10,
            cell: &C3,
        };
        let mut st = MonitorState::default();
        // 4 ms dispatch — within budget.
        C3.max_latency_us.store(4_000, Ordering::Relaxed);
        assert!(check_latency(&s, &mut st).is_none());
        assert_eq!(C3.max_latency_us.load(Ordering::Relaxed), 0, "drained");
        // 25 ms dispatch — fires.
        C3.max_latency_us.store(25_000, Ordering::Relaxed);
        let v = check_latency(&s, &mut st).expect("fires");
        assert_eq!(v.rule, "max-latency-runtime");
        assert_eq!(v.measured, 25);
        assert_eq!(v.declared, 10);
        // Still slow — suppressed; recovery resets.
        C3.max_latency_us.store(30_000, Ordering::Relaxed);
        assert!(check_latency(&s, &mut st).is_none());
        C3.max_latency_us.store(1_000, Ordering::Relaxed);
        assert!(check_latency(&s, &mut st).is_none());
        C3.max_latency_us.store(30_000, Ordering::Relaxed);
        assert!(check_latency(&s, &mut st).is_some());
    }
}
