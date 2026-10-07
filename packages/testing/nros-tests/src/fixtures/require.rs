//! The ONE conversion from a fixture-resolver `Err` to a test outcome.
//!
//! issue 1129 / phase-450 W1.
//!
//! # Why this exists
//!
//! Every `build_*` resolver returns `TestResult<&Path>`, and until this module
//! existed each of its **260 call sites across 49 files** decided for itself
//! what an `Err` meant. They decided it in prose:
//!
//! ```ignore
//! .unwrap_or_else(|e| panic!("riscv-nuttx C talker not built: {e}"))
//! .expect("native listener fixture not prebuilt")
//! ```
//!
//! Two things follow from that, and both were measured.
//!
//! **The decision could not be audited.** `check-skip-budget` exists to catch a
//! resolver error being laundered into a skip, and the only thing it could key
//! on was the wording — so it grepped `not prebuilt` and saw **4 of 61** sites,
//! because 57 said `not built`. Widening the regex was the wrong fix and the
//! phase doc said so before it was written: *"this is the canonical fix the
//! class, not the site case, so it is also the one that must not be fixed by
//! adding a second spelling to the matcher."* A 62nd site with new wording
//! re-opens it either way.
//!
//! **The decision was re-made 260 times.** Skip-or-fail is one rule, and a rule
//! restated at every call site is a rule with 260 chances to differ. One file
//! had already noticed and written a private `skip_missing_fixture`; it was
//! used by that file alone, and it matched on prose too.
//!
//! # The shape of the fix
//!
//! The distinction moves into the TYPE — `TestError::FixtureNotBuilt` — which
//! is the rule `TestError::RouterUnavailable` already states one variant up:
//! *a caller can only make that distinction if the type carries it.* Then this
//! is the only place that reads it, and the gate keys on call sites of this
//! helper rather than on anybody's wording.
//!
//! # Skip or fail
//!
//! `FixtureNotBuilt` skips; everything else panics. That is not a softening:
//! a run that PROMISED its fixtures never reaches here, because
//! `require_prebuilt_binary_checks` panics outright when `gate_promised_fixtures()`
//! — a broken promise is a failure, not an environment skip. So the only way to
//! arrive here with `FixtureNotBuilt` is an ungated run that built nothing, and
//! there a skip is the honest verdict.
//!
//! A STALE fixture is deliberately not in that set. It stays `BuildFailed` and
//! so panics here, because a stale artifact laundered into a skip is issue
//! 0445's absorbing verdict.

use crate::{TestError, TestResult};

/// The canonical marker every not-built skip carries.
///
/// `check-skip-budget` keys on THIS, one string emitted from one place, rather
/// than on the wording of whichever call site produced it.
pub const FIXTURE_NOT_BUILT_MARKER: &str = "fixture not built";

/// Convert a fixture-resolver `Result` into a value, a skip, or a panic.
pub trait RequireFixture<T> {
    /// Unwrap a `build_*` resolver's result.
    ///
    /// `what` names the fixture for the reader — "native talker", "riscv-nuttx
    /// C talker". It is prose for a human and nothing keys on it.
    fn require(self, what: &str) -> T;
}

impl<T> RequireFixture<T> for TestResult<T> {
    fn require(self, what: &str) -> T {
        match self {
            Ok(v) => v,
            Err(TestError::FixtureNotBuilt(msg)) => {
                crate::skip!("{FIXTURE_NOT_BUILT_MARKER}: {what}: {msg}");
            }
            Err(other) => panic!("failed to resolve the {what} fixture: {other:?}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The verdict `require` reaches for `r`, as a cell loop sees it: the
    /// panic payload, sorted by `skip_marker::is_skip` exactly as the five
    /// `lane_scope::CONSUMERS` sort a caught cell.
    fn verdict(r: TestResult<()>) -> (bool, String) {
        let got = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| r.require("probe")));
        let payload = got.expect_err("an Err must never yield a value");
        let msg = payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_default();
        (crate::skip_marker::is_skip(&msg), msg)
    }

    /// Issue 1729 — a STALE in-lane fixture is a FAILURE. The consolidated
    /// matrix consumers used to turn every resolver `Err` into a local
    /// `skip!`, so `sched_dims_applied` PASSED with three in-lane Zephyr cells
    /// reading "realtime fixture unavailable: … is STALE" and its derived-tier
    /// cell — the one that was actually broken — never booted.
    #[test]
    fn a_stale_fixture_is_a_failure_not_a_skip() {
        let (skip, msg) = verdict(Err(TestError::BuildFailed(
            "Zephyr fixture is STALE — a source is newer than the built binary".into(),
        )));
        assert!(!skip, "a STALE fixture was classified as a skip: {msg}");
        assert!(msg.contains("STALE"), "the reason is lost: {msg}");
    }

    /// The other arm, so the test above cannot pass by `require` panicking
    /// for everything: an UNGATED not-built fixture stays a skip (issue 0584
    /// part 2 — a gated run never gets here, its resolver panics first).
    #[test]
    fn an_ungated_unbuilt_fixture_is_a_skip() {
        let (skip, msg) = verdict(Err(TestError::FixtureNotBuilt("not prebuilt".into())));
        assert!(skip, "an ungated unbuilt fixture was not a skip: {msg}");
        assert!(msg.contains(FIXTURE_NOT_BUILT_MARKER), "{msg}");
    }
}
