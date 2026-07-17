//! Shared glue for the contract-monitor parity fixture bins.
//!
//! RFC-0052 / phase-296 W3b.4/.5. The three bins (`pub`, `sub`, `diagsink`)
//! all drain the executor's contract-violation ring through the
//! `nros-diagnostics` reporter and publish the resulting `DiagnosticArray`
//! on `/diagnostics`. The rule-id vocabulary is the SAME strings play_launch
//! enforces on the Linux runtime (RFC-0050), so the same contract file yields
//! the same rule on either runtime.

use std::{sync::OnceLock, time::Instant};

use nros::{Executor, monitor::Violation};
use nros_diagnostics::{
    ContractKind, DiagnosticArray, DiagnosticReporter, RULE_MAX_AGE, RULE_MAX_LATENCY,
    RULE_RATE_HIERARCHY, Severity,
};

/// The parity topic the pub publishes and the sub subscribes. A leading
/// `std_msgs/Header` so the sub-side age monitor can peek `header.stamp`.
pub const HEADER_TOPIC: &str = "/cm_header";

/// Where both monitor sides publish their violation reports.
pub const DIAG_TOPIC: &str = "/diagnostics";

/// Declared publisher rate guarantee (milli-Hz): 10 Hz.
pub const MIN_RATE_HZ_MILLI: u32 = 10_000;

/// Declared subscriber max data age (ms).
pub const MAX_AGE_MS: u32 = 200;

/// Process-monotonic microseconds (the reporter's rate-limit clock).
pub fn now_us() -> u64 {
    static BASE: OnceLock<Instant> = OnceLock::new();
    BASE.get_or_init(Instant::now).elapsed().as_micros() as u64
}

/// Classify a rule id into the contract side it belongs to (drives the
/// diagnosis quadrant): rate/latency are publisher/path GUARANTEES, age is
/// a subscriber ASSUMPTION.
fn kind_for(rule: &str) -> ContractKind {
    match rule {
        RULE_MAX_AGE => ContractKind::Assumption,
        _ => ContractKind::Guarantee,
    }
}

/// Map a drained [`Violation`] to a `DiagnosticArray`, or `None` while the
/// reporter is rate-limited.
pub fn violation_to_report(
    reporter: &mut DiagnosticReporter,
    v: &Violation,
) -> Option<DiagnosticArray> {
    let mut message = heapless::String::<64>::new();
    // `measured`/`declared` units differ per rule; the diagsink keys only on
    // the rule id + hardware_id, so a compact human message is enough here.
    use core::fmt::Write as _;
    let _ = write!(
        message,
        "measured {} vs declared {}",
        v.measured, v.declared
    );
    reporter.report(
        now_us(),
        rule_const(v.rule),
        Severity::Error,
        kind_for(v.rule),
        v.fqn,
        &message,
    )
}

/// Normalize the executor's rule string to the reporter's `&'static` const
/// (identity for the three W3b rules; keeps the vocabulary pinned).
fn rule_const(rule: &str) -> &'static str {
    match rule {
        RULE_RATE_HIERARCHY => RULE_RATE_HIERARCHY,
        RULE_MAX_AGE => RULE_MAX_AGE,
        RULE_MAX_LATENCY => RULE_MAX_LATENCY,
        _ => "deadline-miss-runtime",
    }
}

/// Drain every pending violation from the executor and hand each report to
/// `publish`. Returns the number of reports emitted this call.
pub fn drain_and_report(
    executor: &mut Executor,
    reporter: &mut DiagnosticReporter,
    mut publish: impl FnMut(&DiagnosticArray),
) -> usize {
    let mut reports: heapless::Vec<DiagnosticArray, 8> = heapless::Vec::new();
    executor.drain_violations(|v| {
        if let Some(arr) = violation_to_report(reporter, v) {
            let _ = reports.push(arr);
        }
    });
    let n = reports.len();
    for arr in &reports {
        publish(arr);
    }
    n
}
