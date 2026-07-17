//! RFC-0052 / phase-296 W3b.1 — the on-target contract-violation reporter.
//!
//! Thin-wrapper discipline (RFC-0019): this crate BUILDS `DiagnosticArray`
//! messages — it owns no publisher, no executor, no aggregation. The entry
//! glue (fixture, codegen entry, `run_tiers` loop) creates one
//! `Publisher<DiagnosticArray>` on `/diagnostics` and publishes whatever
//! [`DiagnosticReporter::report`] returns. `no_std` + heapless throughout.
//!
//! Rule ids are the play_launch runtime-enforcement vocabulary — the SAME
//! contract violated on either runtime reports in the SAME words
//! (cross-runtime parity, RFC-0050/0052).

#![no_std]

pub use nros_diagnostic_msgs::msg::{DiagnosticArray, DiagnosticStatus, KeyValue};

/// Publisher rate below the declared `min_rate_hz` (pub-endpoint guarantee).
pub const RULE_RATE_HIERARCHY: &str = "rate-hierarchy-runtime";
/// Message age above the declared `max_age_ms` (sub-endpoint assumption).
pub const RULE_MAX_AGE: &str = "max-age-runtime";
/// Path latency above the declared `max_latency_ms` (path guarantee).
pub const RULE_MAX_LATENCY: &str = "max-latency-runtime";

/// Which side of the contract the violated field belongs to — drives the
/// 4-quadrant diagnosis (RFC-0050 §contracts): a violated GUARANTEE with
/// met assumptions is a node bug; a violated ASSUMPTION is an upstream
/// problem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractKind {
    Assumption,
    Guarantee,
}

impl ContractKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            ContractKind::Assumption => "assumption",
            ContractKind::Guarantee => "guarantee",
        }
    }
}

/// Violation severity → `DiagnosticStatus` level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Warn,
    Error,
}

impl Severity {
    /// `DiagnosticStatus` level bytes (generated consts are module-private;
    /// values are the ROS-fixed 0=OK 1=WARN 2=ERROR 3=STALE).
    pub const fn level(self) -> u8 {
        match self {
            Severity::Warn => 1,
            Severity::Error => 2,
        }
    }
}

/// Builds rate-limited `DiagnosticArray` entries. One reporter per node
/// (or per entry) is plenty — it carries only the rate-limit state.
#[derive(Debug, Default)]
pub struct DiagnosticReporter {
    /// Minimum µs between emitted reports (monotonic clock supplied by the
    /// caller). 0 = unlimited.
    pub min_interval_us: u64,
    last_report_us: u64,
}

impl DiagnosticReporter {
    pub const fn new(min_interval_us: u64) -> Self {
        Self {
            min_interval_us,
            last_report_us: 0,
        }
    }

    /// Build one violation report, or `None` while rate-limited.
    ///
    /// `now_us` is the caller's monotonic clock (the executor's `clock_us`
    /// on target). `fqn` names the violating entity (node FQN or
    /// `node/endpoint` ref — the same key shape the SystemModel uses).
    /// The stamp is left zero; the transport-side observer keys on
    /// content, and stamping would drag the epoch clock into `no_std`
    /// paths that don't have one.
    pub fn report(
        &mut self,
        now_us: u64,
        rule_id: &str,
        severity: Severity,
        kind: ContractKind,
        fqn: &str,
        message: &str,
    ) -> Option<DiagnosticArray> {
        if self.min_interval_us > 0
            && self.last_report_us != 0
            && now_us.saturating_sub(self.last_report_us) < self.min_interval_us
        {
            return None;
        }
        self.last_report_us = now_us;

        let mut status = DiagnosticStatus {
            level: severity.level(),
            ..Default::default()
        };
        let _ = status.name.push_str(rule_id);
        let _ = status.message.push_str(message);
        let _ = status.hardware_id.push_str(fqn);
        let mut kv = KeyValue::default();
        let _ = kv.key.push_str("kind");
        let _ = kv.value.push_str(kind.as_str());
        let _ = status.values.push(kv);

        let mut arr = DiagnosticArray::default();
        let _ = arr.status.push(status);
        Some(arr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_carries_rule_vocabulary_and_kind() {
        let mut r = DiagnosticReporter::new(0);
        let arr = r
            .report(
                1,
                RULE_RATE_HIERARCHY,
                Severity::Error,
                ContractKind::Guarantee,
                "/ctrl/control_node/cmd",
                "measured 1.0 Hz < declared min_rate_hz (100)",
            )
            .expect("first report always emits");
        let st = &arr.status[0];
        assert_eq!(st.name.as_str(), "rate-hierarchy-runtime");
        assert_eq!(st.hardware_id.as_str(), "/ctrl/control_node/cmd");
        assert_eq!(st.level, 2, "ERROR level");
        assert_eq!(st.values[0].key.as_str(), "kind");
        assert_eq!(st.values[0].value.as_str(), "guarantee");
    }

    #[test]
    fn rate_limit_suppresses_then_allows() {
        let mut r = DiagnosticReporter::new(1_000_000);
        assert!(r
            .report(
                10,
                RULE_MAX_AGE,
                Severity::Warn,
                ContractKind::Assumption,
                "/a",
                "m"
            )
            .is_some());
        assert!(r
            .report(
                500_000,
                RULE_MAX_AGE,
                Severity::Warn,
                ContractKind::Assumption,
                "/a",
                "m"
            )
            .is_none());
        assert!(r
            .report(
                1_100_000,
                RULE_MAX_AGE,
                Severity::Warn,
                ContractKind::Assumption,
                "/a",
                "m"
            )
            .is_some());
    }
}
