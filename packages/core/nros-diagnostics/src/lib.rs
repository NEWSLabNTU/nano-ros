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

#![forbid(unsafe_code)]
#![no_std]

pub use nros_diagnostic_msgs::msg::{DiagnosticArray, DiagnosticStatus, KeyValue};

/// Publisher rate below the declared `min_rate_hz` (pub-endpoint guarantee).
pub const RULE_RATE_HIERARCHY: &str = "rate-hierarchy-runtime";
/// Message age above the declared `max_age_ms` (sub-endpoint assumption).
pub const RULE_MAX_AGE: &str = "max-age-runtime";
/// Path latency above the declared `max_latency_ms` (path guarantee).
pub const RULE_MAX_LATENCY: &str = "max-latency-runtime";
/// phase-462 W2 -- a contracted subscription that took NOTHING for a whole
/// `max_age_ms` window (sub-endpoint assumption, the on-target form of a
/// liveliness lease).
///
/// Distinct from [`RULE_MAX_AGE`], and it has to be: that rule judges a
/// message that ARRIVED and was too old, so an input that stops entirely
/// leaves it silent forever -- the very failure a `max_age_ms` promise is
/// made against. A declared bound on how old data may be is also a bound on
/// how long there may be none.
pub const RULE_SILENCE: &str = "silence-runtime";

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

/// Issue 1635 — which side of the contract a runtime rule judges.
///
/// The two rules about what ARRIVES (`max-age-runtime`, `silence-runtime`) are
/// subscriber ASSUMPTIONS; every other rule judges what this image itself does
/// (its publish rate, its path latency, its deadlines, its own stack) and is a
/// GUARANTEE. One table, so the fixture, the C/C++ entries and a Rust entry
/// cannot classify the same rule two ways.
pub fn kind_for_rule(rule: &str) -> ContractKind {
    match rule {
        RULE_MAX_AGE | RULE_SILENCE => ContractKind::Assumption,
        _ => ContractKind::Guarantee,
    }
}

impl DiagnosticReporter {
    /// Issue 1635 — one drained executor violation, as a `DiagnosticArray`
    /// (or `None` while rate-limited). The ONE mapping from the executor's
    /// `Violation` fields to a report; `nros-node`'s type is not named here so
    /// this crate stays below it.
    ///
    /// `rule` keeps the executor's own spelling — it is already the
    /// play_launch vocabulary — and the message carries both numbers, whose
    /// unit is the rule's (milli-Hz for rate, ms for age/latency, us for a
    /// deadline miss).
    pub fn report_violation(
        &mut self,
        now_us: u64,
        rule: &str,
        fqn: &str,
        measured: u32,
        declared: u32,
    ) -> Option<DiagnosticArray> {
        use core::fmt::Write as _;
        let mut message = heapless::String::<64>::new();
        let _ = write!(message, "measured {measured} vs declared {declared}");
        self.report(
            now_us,
            rule,
            Severity::Error,
            kind_for_rule(rule),
            fqn,
            &message,
        )
    }
}

/// phase-474 I7 -- one violation report, streamed as CDR straight into `w`,
/// byte-identical to serializing what [`DiagnosticReporter::report_violation`]
/// returns (with no rate limit).
///
/// The reporter runs on the spin thread, at detection. The value form puts a
/// whole `DiagnosticArray` on that stack -- 5,176 B on a 64-bit host: a
/// four-slot vector of 1,224 B statuses, each with eight key/value slots,
/// plus the status built before it -- to send one status with one key/value.
/// On the safety island that frame ran a 16 KiB main stack into the idle
/// thread's on the first stored violation. This writes the same bytes from the
/// borrowed strings; the largest locals are the 64 B message and an empty
/// `Header` (frame id capacity 256).
///
/// Field limits follow the value form exactly: a string longer than its
/// field's capacity (name 64, message 128, hardware id 96) is sent EMPTY,
/// because that is what `heapless::String::push_str` leaves.
pub fn write_violation_report(
    w: &mut nros_serdes::CdrWriter<'_>,
    rule: &str,
    fqn: &str,
    measured: u32,
    declared: u32,
) -> Result<(), nros_serdes::SerError> {
    use core::fmt::Write as _;
    use nros_serdes::Serialize as _;
    fn fit(s: &str, cap: usize) -> &str {
        if s.len() <= cap {
            s
        } else {
            ""
        }
    }
    let mut message = heapless::String::<64>::new();
    let _ = write!(message, "measured {measured} vs declared {declared}");
    // DiagnosticArray { header, status: [one] }
    let array = w.begin_dheader()?;
    nros_std_msgs::msg::Header::default().serialize(w)?;
    w.write_u32(1)?;
    // DiagnosticStatus { level, name, message, hardware_id, values: [one] }
    let status = w.begin_dheader()?;
    w.write_u8(Severity::Error.level())?;
    w.write_string(fit(rule, 64))?;
    w.write_string(fit(&message, 128))?;
    w.write_string(fit(fqn, 96))?;
    w.write_u32(1)?;
    // KeyValue { "kind", assumption | guarantee }
    let kv = w.begin_dheader()?;
    w.write_string("kind")?;
    w.write_string(kind_for_rule(rule).as_str())?;
    w.end_dheader(kv)?;
    w.end_dheader(status)?;
    w.end_dheader(array)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// phase-474 I7 -- the streamed report is the value form's bytes, under
    /// XCDR1 and XCDR2, including a string past its field's capacity.
    #[test]
    fn the_streamed_report_is_the_value_reports_bytes() {
        use nros_serdes::{CdrWriter, Serialize as _};
        let long = "/a/very/long/node/name/that/does/not/fit/in/ninety/six/bytes/of/hardware/id/at/all/really/truly/not/even/close";
        assert!(long.len() > 96);
        let cases = [
            (RULE_SILENCE, "/n/in", 0u32, 200u32),
            (RULE_MAX_LATENCY, "/mrm_handler/mrm_state", 250, 206),
            (RULE_RATE_HIERARCHY, long, 9_984, 10_000),
        ];
        for xcdr2 in [false, true] {
            for (rule, fqn, m, d) in cases {
                let mut a = [0u8; 512];
                let mut b = [0u8; 512];
                let mut wa = if xcdr2 {
                    CdrWriter::new_with_header_xcdr2(&mut a).unwrap()
                } else {
                    CdrWriter::new_with_header(&mut a).unwrap()
                };
                DiagnosticReporter::new(0)
                    .report_violation(0, rule, fqn, m, d)
                    .unwrap()
                    .serialize(&mut wa)
                    .unwrap();
                let la = wa.position();
                let mut wb = if xcdr2 {
                    CdrWriter::new_with_header_xcdr2(&mut b).unwrap()
                } else {
                    CdrWriter::new_with_header(&mut b).unwrap()
                };
                write_violation_report(&mut wb, rule, fqn, m, d).unwrap();
                let lb = wb.position();
                assert_eq!(a[..la], b[..lb], "xcdr2={xcdr2} {rule} {fqn}");
            }
        }
    }

    #[test]
    fn a_violation_report_keeps_the_rule_and_classifies_its_side() {
        let mut r = DiagnosticReporter::new(0);
        let arr = r
            .report_violation(1, RULE_SILENCE, "/n/in", 0, 200)
            .expect("first report always emits");
        let st = &arr.status[0];
        assert_eq!(st.name.as_str(), RULE_SILENCE);
        assert_eq!(st.hardware_id.as_str(), "/n/in");
        assert_eq!(st.message.as_str(), "measured 0 vs declared 200");
        assert_eq!(st.values[0].value.as_str(), "assumption");
        assert_eq!(kind_for_rule(RULE_RATE_HIERARCHY), ContractKind::Guarantee);
        assert_eq!(
            kind_for_rule("deadline-miss-runtime"),
            ContractKind::Guarantee
        );
        assert_eq!(kind_for_rule(RULE_MAX_AGE), ContractKind::Assumption);
    }

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
