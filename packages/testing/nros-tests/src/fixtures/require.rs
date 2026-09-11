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
