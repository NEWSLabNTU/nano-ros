//! Does THIS run's lane admit a given platform? — issue 0571.
//!
//! `scripts/test/lane-filter.sh native` narrows a tier-1 run by excluding test
//! BINARY names and TEST names that carry a platform token. Issue 0357 already
//! recorded that binary exclusion alone is not enough, because the matrix
//! consumers put every platform's cases in one generically-named binary, and
//! added the test-name exclusion to cover it.
//!
//! Four consumers escape BOTH halves, because consolidation went one step
//! further: `entry_e2e entry_matrix`, `realtime_tiers_e2e realtime_tiers`,
//! `multihost_e2e multihost` and `roundtrip_xprocess_e2e roundtrip_xprocess`
//! are ONE test each, iterating every platform cell in a single process. No
//! name filter can reach inside a test, so on a tier-1 host every image that
//! happens to exist gets booted — and every image that does NOT exist is a
//! silently absent cell, which is how `realtime_tiers` reported a 12-second
//! PASS while running 1 of its 16 rows.
//!
//! So the narrowing happens where the platform is actually known: in the cell
//! list. This is the run-scope twin of
//! [`crate::fixtures::lane::require_coord_in_lane`], which does the same job
//! for tier 2's coordinate scoping — same principle (issue 0482): a lane that
//! cannot be expressed as a name filter must be applied at the point where the
//! test binds to a platform.
//!
//! **`NROS_TEST_SCOPE` unset means ALL.** Tier 2/3 and a bare `cargo nextest`
//! run everything, exactly as today — this module only ever narrows a run that
//! explicitly asked to be narrowed.

use crate::matrix::PlatformId;

/// The env var `just ci` sets for a host-only run (`justfile`'s tier-1 recipe).
pub const SCOPE_ENV: &str = "NROS_TEST_SCOPE";

/// Whether this run's lane admits `platform`.
///
/// `NROS_TEST_SCOPE=native` ⇒ only the host board. Anything else — including
/// unset, empty, and `all` — admits everything.
pub fn admits(platform: PlatformId) -> bool {
    scope_admits(std::env::var(SCOPE_ENV).ok().as_deref(), platform)
}

/// [`admits`] without the environment, so both arms are testable in one process
/// (the mistake `fixtures::lane` documents: a `OnceLock`-latched env read can
/// only ever be exercised in one direction per test binary).
pub fn scope_admits(scope: Option<&str>, platform: PlatformId) -> bool {
    match scope.map(str::trim) {
        Some("native") => matches!(platform, PlatformId::Linux),
        _ => true,
    }
}

/// The one-line reason a cell was dropped, for the summary a consumer prints.
///
/// Never silent: a lane that skips 15 of 16 rows must SAY so, or the green is
/// indistinguishable from a green that ran them (issue 0445's rule, and the
/// half of 0571 that made a red cell invisible for months).
pub fn skip_note(platform: PlatformId, lang: &str) -> String {
    format!(
        "{}/{lang}: out of lane ({SCOPE_ENV}=native admits the host board only)",
        platform.just_module()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_scope_admits_only_the_host_board() {
        assert!(scope_admits(Some("native"), PlatformId::Linux));
        for p in [
            PlatformId::NuttxArm,
            PlatformId::NuttxRiscv,
            PlatformId::FreertosMps2,
            PlatformId::ZephyrNativeSim,
            PlatformId::ThreadxRiscv64,
            PlatformId::Esp32Qemu,
        ] {
            assert!(
                !scope_admits(Some("native"), p),
                "{p:?} must not run in a host-only lane"
            );
        }
    }

    /// ThreadX-Linux is a HOSTED simulation, but it is still its own board with
    /// its own fixture, and `lane-filter.sh` excludes it from the native lane by
    /// its `threadx` token. Agreeing with that filter is the point — two
    /// spellings of "what tier 1 runs" is the drift this exists to remove.
    #[test]
    fn hosted_threadx_is_not_the_host_board() {
        assert!(!scope_admits(Some("native"), PlatformId::ThreadxLinux));
    }

    #[test]
    fn unscoped_and_all_admit_everything() {
        for scope in [None, Some(""), Some("all")] {
            for p in [PlatformId::Linux, PlatformId::NuttxArm, PlatformId::Px4] {
                assert!(
                    scope_admits(scope, p),
                    "scope {scope:?} must not narrow {p:?}"
                );
            }
        }
    }
}
