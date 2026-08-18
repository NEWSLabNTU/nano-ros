//! RFC-0078 — a declared execution-time bound, keyed on a named measurement
//! profile.
//!
//! This is the DECLARATION half. Issue 0403 defines the measurement artifact
//! (`nros.wcet.measurements/1`); the two are different objects and keeping them
//! apart is the spine of the design. A measurement is evidence produced by a
//! bench on one part; a declaration is a claim a project makes about a context,
//! reviewable on its own terms.
//!
//! # A measured maximum is NOT a WCET, and this module refuses to pretend
//!
//! The literature is unambiguous: the longest time observed over N runs is a
//! **high-water mark**, not a bound. "As it is generally impossible to observe
//! all potential executions of a real-world program, this approach cannot
//! provide any guarantees about the calculated WCET estimate" (Wilhelm et al.,
//! *The Worst-Case Execution Time Problem*). Measurement-based estimates are
//! optimistic by an unknown amount.
//!
//! An earlier version of this module converted `max_cycles` straight into
//! `exec_ms` and called the result a WCET. That is issue 0259's failure wearing
//! better clothes: 0259 is about an ABSENT number counted as zero, and this
//! would have been an UNDER-estimate counted as measured. Both make a chain
//! look more feasible than the evidence supports, and the second is worse for
//! being hard to see.
//!
//! So the observation and the bound are different fields, and observation alone
//! yields NOTHING:
//!
//! * `max_observed_cycles` — what the bench saw. Evidence, never a bound.
//! * `bound_cycles` — an explicit upper bound, e.g. from static analysis.
//! * `margin_percent` — the industrial practice of inflating the high-water
//!   mark (commonly ~20%, a figure with "very little justification … save for
//!   historical confidence"). Declaring it is how a project says which
//!   unjustified number it chose.
//!
//! A profile that declares neither a bound nor a margin produces no `exec_ms`.
//! That is the point: it still records the measurement, and it refuses to let
//! the measurement masquerade as a guarantee.
//!
//! # Why cycles, converted here
//!
//! `ros-launch-manifest-sched` takes `MapperPath::exec_ms` in MILLISECONDS by
//! cross-repo design agreement ("no invented WCET"), while the bench measures
//! cycles. So a declaration carries cycles PLUS the profile's `clock_hz` and
//! the conversion happens here — a named, testable step rather than arithmetic
//! in a human's head. Eclipse AMALTHEA/APP4MC reached the same shape
//! independently: demand is declared in ticks, and "to obtain execution times,
//! the number of ticks is divided by the individual frequency of each
//! processing unit."
//!
//! # The invariant that outranks the rest
//!
//! **Absent stays representable and stays the DEFAULT.** No boundary acquires a
//! bound by omission, inheritance, or a family fallback.
//! `ChainFeasibleWithoutWcet` firing for an undeclared boundary is the CORRECT
//! output; success is that it names fewer boundaries over time, never that it
//! goes quiet.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// What a bench observed at one boundary, plus optionally a BOUND derived from
/// it.
///
/// The two are separate fields on purpose. `max_observed_cycles` is evidence;
/// `bound_cycles` is a claim. Conflating them is what lets a high-water mark
/// become a scheduling input without anyone deciding that it should.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundaryWcet {
    /// Shortest run seen. Carried for audit: a maximum with no spread is not
    /// reviewable.
    pub min_observed_cycles: u64,
    /// Longest run seen — the high-water mark. NOT a bound.
    pub max_observed_cycles: u64,
    /// How many runs the maximum is a maximum OF. A high-water mark over 3
    /// samples and over 3 million are different claims.
    pub iterations: u64,
    /// An explicit upper bound, when one exists — typically from static
    /// analysis, which is the only method that can produce a guarantee. When
    /// present this is what converts, and `margin_percent` is not consulted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bound_cycles: Option<u64>,
}

/// A named measurement context, and the boundaries measured in it.
///
/// The name (`stm32f4-168mhz-release`) is the load-bearing part: a bound
/// belongs to a CONTEXT, not to code. A board id would be a profile with one
/// member; a platform family would be a profile that omits the clock rate it
/// needs anyway.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WcetProfile {
    pub cpu: String,
    /// `Option` because issue 0403's artifact cannot read the part's clock and
    /// says so (`convertible_to_time: false`). A profile without it is still
    /// valid and still auditable — it simply yields no `exec_ms`, which is a
    /// different thing from yielding zero.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clock_hz: Option<u64>,
    pub profile: String,
    /// The commit the measurement was taken at. Recorded so a reviewer CAN ask
    /// whether it still describes this callback; RFC-0078 deliberately does not
    /// claim to decide that automatically, because nothing in a file can know.
    pub measured_at_commit: String,
    /// Carried even though 0403's bench refuses to emit measurements from a
    /// dead counter — so a HAND-written declaration cannot quietly claim to be
    /// measured when it was not.
    pub counter_valid: bool,
    /// Where the numbers came from, e.g. `nros.wcet.measurements/1`.
    pub source: String,
    /// Percentage added to `max_observed_cycles` to obtain a bound, when no
    /// `bound_cycles` is declared. ~20% is the common industrial figure, and
    /// the literature is blunt that it has "very little justification … save
    /// for historical confidence". Requiring it to be WRITTEN DOWN does not
    /// make it justified; it makes it visible and reviewable, which is the most
    /// this schema can honestly offer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub margin_percent: Option<f64>,
    /// What the measurement run actually exercised — input classes, cache
    /// state, worst path if known. Free text, because anything structured here
    /// would imply a rigour the bench does not have.
    ///
    /// `None` is an admission, not an omission: measurement-based estimation is
    /// only as good as its coverage, and this tree's bench runs fixed synthetic
    /// inputs. A reviewer reading `None` should discount accordingly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coverage: Option<String>,
    /// Keyed `"<node_fqn>/<path_name>"` — rlm's own boundary identity, so its
    /// `boundaries_without_wcet` list and these declarations join by set
    /// difference rather than by judgement.
    #[serde(default)]
    pub boundaries: BTreeMap<String, BoundaryWcet>,
}

/// Why a declaration was rejected. Each variant is a claim that cannot be true
/// of a measured number, so accepting it would launder a guess into evidence.
#[derive(Clone, Debug, PartialEq)]
pub enum WcetError {
    /// A rate of zero converts every cycle count to infinity.
    ClockRateZero,
    /// 0403's bench refuses to emit measurements when the counter is dead, so
    /// this can only be reached by hand.
    CounterNotValid,
    /// A worst case below the best case is not a measurement of anything.
    MaxBelowMin {
        boundary: String,
        min: u64,
        max: u64,
    },
    /// A maximum over no samples.
    NoIterations { boundary: String },
    /// Not `node/path`, so it can never join rlm's boundary list — a
    /// declaration that silently applies to nothing.
    MalformedBoundaryKey { boundary: String },
    /// A bound below what was already OBSERVED. The observation is a witness:
    /// the code demonstrably took that long at least once.
    BoundBelowObserved {
        boundary: String,
        observed: u64,
        bound: u64,
    },
    /// A margin that shrinks the high-water mark rather than inflating it.
    NegativeMargin { margin_percent: f64 },
}

impl std::fmt::Display for WcetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ClockRateZero => write!(
                f,
                "clock_hz is 0 — every cycle count would convert to an infinite exec_ms"
            ),
            Self::CounterNotValid => write!(
                f,
                "counter_valid is false — the bench refuses to emit measurements from a dead \
                 cycle counter, so these numbers were not produced by it (issue 0403)"
            ),
            Self::MaxBelowMin { boundary, min, max } => write!(
                f,
                "{boundary}: max_observed_cycles {max} is below min_observed_cycles {min} \
                 — not a measurement"
            ),
            Self::NoIterations { boundary } => {
                write!(f, "{boundary}: iterations is 0 — a maximum over no samples")
            }
            Self::MalformedBoundaryKey { boundary } => write!(
                f,
                "{boundary}: not `node/path`, so it can never match a boundary rlm reports \
                 and would apply to nothing"
            ),
            Self::BoundBelowObserved {
                boundary,
                observed,
                bound,
            } => write!(
                f,
                "{boundary}: bound_cycles {bound} is below max_observed_cycles {observed} \
                 — the code was already seen taking longer than the bound claims it can"
            ),
            Self::NegativeMargin { margin_percent } => write!(
                f,
                "margin_percent {margin_percent} is negative — a margin inflates the \
                 high-water mark, it does not shrink it"
            ),
        }
    }
}

impl WcetProfile {
    /// Every reason this declaration cannot be believed, not just the first.
    ///
    /// All of them, because a declaration is reviewed as a whole: reporting one
    /// error per edit-and-retry cycle is how a reviewer gives up and stops
    /// reading.
    pub fn validate(&self) -> Vec<WcetError> {
        let mut out = Vec::new();
        if self.clock_hz == Some(0) {
            out.push(WcetError::ClockRateZero);
        }
        if !self.counter_valid {
            out.push(WcetError::CounterNotValid);
        }
        if let Some(m) = self.margin_percent
            && m < 0.0
        {
            out.push(WcetError::NegativeMargin { margin_percent: m });
        }
        for (boundary, b) in &self.boundaries {
            // The key is `<node_fqn>/<path_name>`, and a node FQN is a ROS
            // name: it BEGINS with `/` and may carry namespaces, so
            // `/perception/front/on_scan` is node `/perception/front`, path
            // `on_scan`. An earlier version required exactly two segments and
            // would have rejected every real boundary in the tree — caught by
            // testing against `mapper_input`'s own fixture.
            let malformed = match boundary.rsplit_once('/') {
                Some((node, path)) => node.is_empty() || path.is_empty() || node == "/",
                None => true,
            };
            if malformed {
                out.push(WcetError::MalformedBoundaryKey {
                    boundary: boundary.clone(),
                });
            }
            if b.max_observed_cycles < b.min_observed_cycles {
                out.push(WcetError::MaxBelowMin {
                    boundary: boundary.clone(),
                    min: b.min_observed_cycles,
                    max: b.max_observed_cycles,
                });
            }
            if b.iterations == 0 {
                out.push(WcetError::NoIterations {
                    boundary: boundary.clone(),
                });
            }
            if let Some(bound) = b.bound_cycles
                && bound < b.max_observed_cycles
            {
                out.push(WcetError::BoundBelowObserved {
                    boundary: boundary.clone(),
                    observed: b.max_observed_cycles,
                    bound,
                });
            }
        }
        out
    }

    /// Whether cycles can become milliseconds at all.
    ///
    /// Mirrors the artifact's own `convertible_to_time` field. A consumer that
    /// needs `ms` must check this rather than assume a rate — inventing one is
    /// the manufactured-WCET failure issue 0404 exists to prevent.
    #[must_use]
    pub fn is_convertible_to_time(&self) -> bool {
        matches!(self.clock_hz, Some(hz) if hz > 0)
    }

    /// This boundary's declared BOUND in cycles, or `None` when the declaration
    /// only carries an observation.
    ///
    /// `bound_cycles` wins when present — an explicit bound, typically from
    /// static analysis, is a stronger claim than an inflated high-water mark
    /// and should not be silently overridden by a margin.
    #[must_use]
    pub fn bound_cycles(&self, boundary: &str) -> Option<u64> {
        let b = self.boundaries.get(boundary)?;
        if let Some(explicit) = b.bound_cycles {
            return Some(explicit);
        }
        let margin = self.margin_percent?;
        let inflated = (b.max_observed_cycles as f64) * (1.0 + margin / 100.0);
        Some(inflated.ceil() as u64)
    }

    /// This boundary's `exec_ms`, or `None`.
    ///
    /// `None` covers four situations on purpose, and none of them is zero: the
    /// boundary is not declared, the profile has no clock rate, the rate is
    /// unusable, or — the one this module exists to enforce — the declaration
    /// carries an OBSERVATION but no bound. The caller propagates `None` into
    /// `MapperPath::exec_ms`, where rlm counts the boundary as undeclared and
    /// says so.
    #[must_use]
    pub fn exec_ms(&self, boundary: &str) -> Option<f64> {
        let hz = self.clock_hz.filter(|hz| *hz > 0)?;
        let cycles = self.bound_cycles(boundary)?;
        Some((cycles as f64) / (hz as f64) * 1000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observed(min: u64, max: u64, iters: u64) -> BoundaryWcet {
        BoundaryWcet {
            min_observed_cycles: min,
            max_observed_cycles: max,
            iterations: iters,
            bound_cycles: None,
        }
    }

    fn profile(clock_hz: Option<u64>) -> WcetProfile {
        WcetProfile {
            cpu: "cortex-m4f".into(),
            clock_hz,
            profile: "release".into(),
            measured_at_commit: "a1b2c3d4e5f6".into(),
            counter_valid: true,
            source: "nros.wcet.measurements/1".into(),
            margin_percent: None,
            coverage: None,
            boundaries: BTreeMap::from([(
                "/perception_node/on_scan".to_string(),
                observed(41_120, 68_940, 1000),
            )]),
        }
    }

    /// The finding this redesign exists for: a high-water mark is evidence, not
    /// a bound, and must not reach the scheduler on its own.
    #[test]
    fn an_observation_without_a_bound_yields_no_exec_ms() {
        let p = profile(Some(168_000_000));
        assert!(p.validate().is_empty(), "the declaration itself is fine");
        assert!(p.is_convertible_to_time(), "the rate is there");
        assert_eq!(
            p.exec_ms("/perception_node/on_scan"),
            None,
            "max_observed is a high-water mark; converting it would hand the \
             scheduler an under-estimate wearing the clothes of a measurement"
        );
    }

    #[test]
    fn a_declared_margin_turns_an_observation_into_a_bound() {
        let mut p = profile(Some(1_000_000));
        p.margin_percent = Some(20.0);
        // 68_940 * 1.20 = 82_728 cycles at 1 MHz = 82.728 ms
        assert_eq!(p.bound_cycles("/perception_node/on_scan"), Some(82_728));
        let ms = p.exec_ms("/perception_node/on_scan").unwrap();
        assert!((ms - 82.728).abs() < 1e-6, "got {ms}");
    }

    #[test]
    fn an_explicit_bound_wins_over_a_margin() {
        // Static analysis is the only method that yields a guarantee, so an
        // explicit bound must not be silently overridden by an inflated
        // high-water mark.
        let mut p = profile(Some(1_000_000));
        p.margin_percent = Some(20.0);
        p.boundaries
            .get_mut("/perception_node/on_scan")
            .unwrap()
            .bound_cycles = Some(100_000);
        assert_eq!(p.bound_cycles("/perception_node/on_scan"), Some(100_000));
        let ms = p.exec_ms("/perception_node/on_scan").unwrap();
        assert!((ms - 100.0).abs() < 1e-9, "got {ms}");
    }

    #[test]
    fn a_bound_below_what_was_observed_is_rejected() {
        let mut p = profile(Some(168_000_000));
        p.boundaries
            .get_mut("/perception_node/on_scan")
            .unwrap()
            .bound_cycles = Some(1_000);
        assert!(p.validate().iter().any(|e| matches!(
            e,
            WcetError::BoundBelowObserved { observed, bound, .. }
                if *observed == 68_940 && *bound == 1_000
        )));
    }

    #[test]
    fn a_negative_margin_is_rejected() {
        let mut p = profile(Some(168_000_000));
        p.margin_percent = Some(-10.0);
        assert!(
            p.validate()
                .iter()
                .any(|e| matches!(e, WcetError::NegativeMargin { .. }))
        );
    }

    #[test]
    fn no_clock_rate_yields_no_time_and_never_a_zero() {
        let mut p = profile(None);
        p.margin_percent = Some(20.0);
        assert!(!p.is_convertible_to_time());
        assert_eq!(
            p.exec_ms("/perception_node/on_scan"),
            None,
            "a zero here would claim the callback is free, which is the most \
             optimistic value a bound can take"
        );
        assert!(
            p.validate().is_empty(),
            "not convertible is not unbelievable"
        );
    }

    #[test]
    fn an_undeclared_boundary_is_absent_not_zero() {
        let mut p = profile(Some(168_000_000));
        p.margin_percent = Some(20.0);
        assert_eq!(p.exec_ms("/perception_node/on_image"), None);
        assert_eq!(p.exec_ms("/other_node/on_scan"), None);
        assert_eq!(p.bound_cycles("/perception_node/on_image"), None);
    }

    #[test]
    fn a_zero_rate_is_rejected_rather_than_dividing_by_it() {
        let mut p = profile(Some(0));
        p.margin_percent = Some(20.0);
        assert!(p.validate().contains(&WcetError::ClockRateZero));
        assert!(!p.is_convertible_to_time());
        assert_eq!(p.exec_ms("/perception_node/on_scan"), None);
    }

    #[test]
    fn a_dead_counter_cannot_be_declared_as_measured() {
        let mut p = profile(Some(168_000_000));
        p.counter_valid = false;
        assert!(
            p.validate().contains(&WcetError::CounterNotValid),
            "0403's bench refuses to emit from a dead counter, so this can only \
             have been written by hand"
        );
    }

    #[test]
    fn a_max_below_its_min_is_not_a_measurement() {
        let mut p = profile(Some(168_000_000));
        p.boundaries.insert("/n/p".into(), observed(900, 100, 10));
        assert!(p.validate().iter().any(|e| matches!(
            e,
            WcetError::MaxBelowMin { boundary, .. } if boundary == "/n/p"
        )));
    }

    #[test]
    fn a_maximum_over_no_samples_is_rejected() {
        let mut p = profile(Some(168_000_000));
        p.boundaries.insert("/n/p".into(), observed(10, 20, 0));
        assert!(p.validate().iter().any(|e| matches!(
            e,
            WcetError::NoIterations { boundary } if boundary == "/n/p"
        )));
    }

    #[test]
    fn a_key_that_cannot_join_rlms_boundary_list_is_rejected() {
        for bad in ["on_scan", "trailing/", "/", "/leading"] {
            let mut p = profile(Some(168_000_000));
            p.boundaries.clear();
            p.boundaries.insert(bad.into(), observed(1, 2, 3));
            assert!(
                p.validate().iter().any(|e| matches!(
                    e,
                    WcetError::MalformedBoundaryKey { boundary } if boundary == bad
                )),
                "{bad} should not be accepted as a node/path boundary key"
            );
        }
    }

    #[test]
    fn a_namespaced_node_fqn_is_accepted() {
        // `/perception/front/on_scan` is node `/perception/front`, path
        // `on_scan` — the shape the two-segment rule used to reject.
        let mut p = profile(Some(168_000_000));
        p.boundaries.clear();
        p.boundaries
            .insert("/perception/front/on_scan".into(), observed(1, 2, 3));
        assert!(p.validate().is_empty(), "{:?}", p.validate());
    }

    #[test]
    fn validate_reports_every_reason_not_just_the_first() {
        let mut p = profile(Some(0));
        p.counter_valid = false;
        p.margin_percent = Some(-5.0);
        p.boundaries.insert("bad".into(), observed(9, 1, 0));
        let errs = p.validate();
        assert!(errs.len() >= 5, "expected all reasons, got {errs:?}");
    }

    #[test]
    fn a_declaration_round_trips_through_serde() {
        let mut p = profile(Some(168_000_000));
        p.margin_percent = Some(20.0);
        p.coverage = Some("fixed synthetic inputs; worst path not established".into());
        let json = serde_json::to_string(&p).expect("serialize");
        let back: WcetProfile = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(p, back);
    }
}
