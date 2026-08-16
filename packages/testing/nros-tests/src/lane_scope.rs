//! Does THIS run's lane admit a given platform? — issue 0571.
//!
//! `scripts/test/lane-filter.sh native` narrows a tier-1 run by excluding test
//! BINARY names and TEST names that carry a platform token. Issue 0357 already
//! recorded that binary exclusion alone is not enough, because the matrix
//! consumers put every platform's cases in one generically-named binary, and
//! added the test-name exclusion to cover it.
//!
//! Five consumers escape BOTH halves, because consolidation went one step
//! further: `entry_e2e entry_matrix`, `realtime_tiers_e2e realtime_tiers`,
//! `multihost_e2e multihost`, `roundtrip_xprocess_e2e roundtrip_xprocess` and
//! `sched_dims_applied_e2e sched_dims_applied` are ONE test each, iterating
//! every platform cell in a single process. No name filter can reach inside a
//! test, so on a tier-1 host every image that happens to exist gets booted —
//! and every image that does NOT exist is a silently absent cell, which is how
//! `realtime_tiers` reported a 12-second PASS while running 1 of its 16 rows.
//!
//! The fifth was found the hard way (issue 0630) and is why [`CONSUMERS`] is
//! now DATA with a test behind it rather than a sentence in this paragraph.
//! `sched_dims_applied` is phase-329 W2's consolidation of ten `*_applied.rs`
//! files — the same shape as the other four, created by the same move, and
//! simply not on anyone's list. On a host with no Zephyr workspace it reached
//! its zephyr cells, found no west-built image, and PANICKED rather than
//! skipped, because `NROS_TEST_SCOPE` being set means a gate already promised
//! the fixtures exist (issue 0584). So tier 1 — "the tier anyone can afford
//! per task" — could not go green there at all.
//!
//! That is the issue-0328 shape: a fix applied at the sites where the symptom
//! was seen. The remedy CLAUDE.md prescribes is a gate that covers the class,
//! which is [`tests::every_cell_iterating_test_is_classified`].
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

/// Test files that iterate platform-varying cells in ONE test and must
/// therefore narrow with [`admits`] — a name filter cannot reach inside them.
///
/// Data, not prose, because prose is what let `sched_dims_applied` be the
/// fifth (issue 0630). Paired with [`EXEMPT`] by
/// [`tests::every_cell_iterating_test_is_classified`], which recomputes the
/// candidate set from the sources and refuses anything in neither list.
pub const CONSUMERS: &[&str] = &[
    "entry_e2e.rs",
    "multihost_e2e.rs",
    "realtime_tiers_e2e.rs",
    "roundtrip_xprocess_e2e.rs",
    "sched_dims_applied_e2e.rs",
];

/// Files that read a cell's platform but need no narrowing, each with the
/// reason — an exemption is a claim, and an unexplained one is how a real
/// consumer gets waved through.
pub const EXEMPT: &[(&str, &str)] = &[
    (
        "matrix_fixture_coverage.rs",
        "a coverage GATE over the tables: it reads every cell's platform and          boots nothing, so no lane can make one of them unavailable",
    ),
    (
        "sched_dims_model_coverage.rs",
        "the same, for the sched-dim tables",
    ),
    (
        "native_example_pubsub_e2e.rs",
        "filters to `PlatformId::Linux` before doing anything, so its cell set          is the host board by construction and `admits` would be a no-op",
    ),
    (
        "native_example_reqresp_e2e.rs",
        "the same, for the request/response cells",
    ),
];

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

    /// Issue 0630 — every test that iterates platform-varying cells is either
    /// a [`CONSUMERS`] entry that calls [`admits`], or an [`EXEMPT`] one with a
    /// stated reason. Nothing may be in neither.
    ///
    /// The candidate set is RECOMPUTED from the sources rather than listed
    /// twice: a file that reads a cell list AND reads a cell's `.platform` is a
    /// candidate. That predicate is deliberately generous — it catches the
    /// coverage gates too, which is why `EXEMPT` exists and why each entry
    /// carries a reason. A gate whose candidate set is itself hand-maintained
    /// would be the defect it is checking for (issue 0196: a gate narrower
    /// than the rule it enforces).
    #[test]
    fn every_cell_iterating_test_is_classified() {
        let dir = crate::project_root().join("packages/testing/nros-tests/tests");
        let mut candidates: Vec<(String, String)> = Vec::new();
        for entry in std::fs::read_dir(&dir).expect("tests/ is readable") {
            let path = entry.expect("readable dir entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let Ok(body) = std::fs::read_to_string(&path) else {
                continue;
            };
            let iterates_cells = ["matrix::CELLS", "SCHED_CELLS", "interop::CELLS"]
                .iter()
                .any(|needle| body.contains(needle));
            // `<binding>.platform` — a cell's platform being read, which is what
            // makes a file capable of booting more than one board.
            let reads_platform = body
                .match_indices(".platform")
                .any(|(i, _)| body[..i].ends_with(|c: char| c.is_ascii_alphanumeric() || c == '_'));
            if iterates_cells && reads_platform {
                let name = path
                    .file_name()
                    .expect("a file")
                    .to_string_lossy()
                    .into_owned();
                candidates.push((name, body));
            }
        }
        assert!(
            !candidates.is_empty(),
            "the candidate predicate matched NOTHING — it has rotted, and a \
             gate that matches nothing passes forever"
        );

        let mut unclassified: Vec<&str> = Vec::new();
        let mut silent_consumers: Vec<&str> = Vec::new();
        for (name, body) in &candidates {
            let listed = CONSUMERS.contains(&name.as_str());
            let exempt = EXEMPT.iter().any(|(f, _)| f == name);
            if !listed && !exempt {
                unclassified.push(name);
            }
            if listed && !body.contains("lane_scope::admits") {
                silent_consumers.push(name);
            }
        }

        assert!(
            unclassified.is_empty(),
            "these tests iterate platform-varying cells and are in neither \
             `lane_scope::CONSUMERS` nor `EXEMPT`: {unclassified:?}\n\
             \n\
             A test like this cannot be narrowed by a name filter (issue 0357), \
             so under `NROS_TEST_SCOPE=native` it reaches every platform's \
             cells. A missing non-host fixture then PANICS rather than skips \
             (issue 0584 — the scope var means a gate already promised the \
             fixtures), so tier 1 cannot go green on a host without that \
             platform's toolchain. That is issue 0630.\n\
             \n\
             Add `if !nros_tests::lane_scope::admits(c.platform) {{ … }}` to the \
             cell loop and list it in CONSUMERS — or add it to EXEMPT with the \
             reason it needs no narrowing."
        );
        assert!(
            silent_consumers.is_empty(),
            "these are listed in `lane_scope::CONSUMERS` but never call \
             `lane_scope::admits`: {silent_consumers:?}\n\
             The list is a claim about behaviour; a member that does not narrow \
             makes it a comment."
        );

        // Both lists name real files. A stale entry is how a list stops
        // describing the tree while still passing.
        for name in CONSUMERS {
            assert!(
                dir.join(name).is_file(),
                "lane_scope::CONSUMERS names a file that no longer exists: {name}"
            );
        }
        for (name, why) in EXEMPT {
            assert!(
                dir.join(name).is_file(),
                "lane_scope::EXEMPT names a file that no longer exists: {name}"
            );
            assert!(
                !why.trim().is_empty(),
                "lane_scope::EXEMPT entry {name} has no reason"
            );
        }
    }

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
