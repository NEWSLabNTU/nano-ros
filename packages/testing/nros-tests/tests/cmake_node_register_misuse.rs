//! §212.L.9 cmake-fn reject diagnostics — the configure MUST fail.
//!
//! **The configures run in the BUILD stage** (issue 1620). Each case is a
//! `cmake-configure-verdict` row over `fixtures/cmake_node_register_misuse/`,
//! whose `cases/<row id>.cmake` supplies the call under test; the build stage
//! configures it, records the exit status and stderr, and succeeds whatever
//! cmake said. These tests assert the recorded verdict. (They used to configure
//! at test time as a documented exception to "No compilation inside tests" — a
//! must-fail configure was held to be un-prebuildable, which is true of a
//! configured TREE and false of a recorded VERDICT.) The positive metadata cases
//! were already build-stage fixtures (`cmake_node_register_metadata.rs`).

use nros_tests::{
    TestResult,
    fixtures::{CompileOutcome, require_compile_verdict},
};

fn outcome(id: &str) -> TestResult<CompileOutcome> {
    Ok(require_compile_verdict(id)?.outcome)
}

#[test]
fn nano_ros_node_register_rejects_unqualified_class() -> TestResult<()> {
    // RFC-0057 D2 retired the 212.L.4 pkg-prefix rule (CLASS may carry any
    // upstream namespace); the live rule is that CLASS must still be a
    // namespace-QUALIFIED name — the entry codegen needs a real type name.
    let out = outcome("cmake_register_unqualified_class")?;
    assert!(
        !out.success(),
        "expected cmake configure to fail on an unqualified CLASS"
    );
    assert!(
        out.stderr.contains("must be a") && out.stderr.contains("namespace-qualified"),
        "expected the RFC-0057 qualified-class diagnostic, got:\n{}",
        out.stderr
    );
    Ok(())
}

#[test]
fn nano_ros_application_rejects_embedded_deploy() -> TestResult<()> {
    // `nano_ros_application` (the 212.N.6 shim) was retired in 287-W8; the
    // live spelling of the same misuse is `nano_ros_entry`.
    let out = outcome("cmake_entry_rejects_embedded_deploy")?;
    assert!(
        !out.success(),
        "expected cmake configure to fail on embedded DEPLOY in Application"
    );
    let err = &out.stderr;
    // `nano_ros_application` is now a deprecated shim → `nano_ros_entry`; accept
    // the entry-layer board-centric wording or the legacy L.2 wording.
    assert!(
        err.contains("native-only")
            || err.contains("Phase 212.L.2")
            || err.contains("embedded Entry pkgs need a Board")
            || err.contains("rejected"),
        "expected an embedded-deploy rejection diagnostic, got:\n{err}"
    );
    Ok(())
}

/// `ENTITIES` is out of the GRAMMAR (retired phase-412; removed from every
/// `cmake_parse_arguments` keyword list in the wave that added this test), so
/// the refusal reads `ARGN` instead of a parse result.
///
/// That is what makes the three positions below one test rather than three
/// unrelated ones: with no keyword in the grammar, `ENTITIES` written after a
/// multi-value keyword is SWALLOWED into it (`SOURCES src/dummy.cpp ENTITIES
/// sub:…` would compile a source file named `ENTITIES`), and written anywhere
/// else it lands in `_NRC_UNPARSED_ARGUMENTS`, which nothing reads — a silent
/// drop of a declaration the caller believes is sizing its pools. Both must
/// refuse, and an `IN_LIST ARGN` test is the only thing that catches both.
#[test]
fn entities_is_refused_wherever_a_stale_caller_writes_it() -> TestResult<()> {
    for (case, id) in [
        // swallowed into SOURCES if nothing refuses
        (
            "after a multi-value keyword",
            "cmake_register_entities_after_sources",
        ),
        // lands in UNPARSED_ARGUMENTS, which nothing reads
        (
            "before any multi-value keyword",
            "cmake_register_entities_before_sources",
        ),
        // the valueless form the old KEYWORDS_MISSING_VALUES arm covered
        ("valueless", "cmake_register_entities_valueless"),
    ] {
        let out = outcome(id)?;
        assert!(
            !out.success(),
            "{case}: expected configure to fail on a retired ENTITIES argument"
        );
        let err = &out.stderr;
        assert!(
            err.contains("ENTITIES was retired"),
            "{case}: expected the ENTITIES tombstone, got:\n{err}"
        );
        // The refusal has to name where the facts live now, or it is a remedy
        // the reader cannot follow — which is exactly what three `message()`
        // strings elsewhere in cmake had become, still naming this very
        // argument 24 days after it started raising FATAL_ERROR.
        assert!(
            err.contains("contract.yaml") && err.contains("system.toml"),
            "{case}: the tombstone must name BOTH live surfaces (bringup \
             sidecar, standalone leaf), got:\n{err}"
        );
    }
    Ok(())
}

/// Negative control for the test above. A call with no `ENTITIES` must not
/// reach the tombstone — asserted on the DIAGNOSTIC, not on success, because
/// this harness stages no board and the configure may fail for its own
/// unrelated reasons. A refusal that fires on every call would satisfy the
/// positive cases just as well.
#[test]
fn a_call_without_entities_never_reaches_the_tombstone() -> TestResult<()> {
    let out = outcome("cmake_register_without_entities")?;
    assert!(
        !out.stderr.contains("ENTITIES was retired"),
        "a call with no ENTITIES argument raised the tombstone:\n{}",
        out.stderr
    );
    Ok(())
}
