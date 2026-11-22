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
    // ~~`nano_ros_application`~~ (the 212.N.6 shim) was retired in 287-W8; the
    // live spelling of the same misuse is `nano_ros_entry`.
    let out = outcome("cmake_entry_rejects_embedded_deploy")?;
    assert!(
        !out.success(),
        "expected cmake configure to fail on embedded DEPLOY in Application"
    );
    let err = &out.stderr;
    // ~~`nano_ros_application`~~ is now a deprecated shim → `nano_ros_entry`; accept
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
/// else it lands in _NRC_UNPARSED_ARGUMENTS, which nothing reads — a silent
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

/// phase-583 W1 (RFC-0071 D20) -- one carrier per EXECUTABLE. A package that
/// registers two components under two EXECUTABLE names builds two carriers;
/// neither is refused. Issue 2744 refused the second, because each road used to
/// build one carrier per package.
#[test]
fn two_executables_in_one_package_each_claim_a_carrier() -> TestResult<()> {
    let out = outcome("cmake_carrier_two_executables_build")?;
    assert!(
        out.success() && out.stdout.contains("both executables claimed a carrier"),
        "two EXECUTABLE names must each build a carrier:\n{}\n{}",
        out.stdout,
        out.stderr
    );
    Ok(())
}

/// phase-583 W1 -- each EXECUTABLE is one program. Two components registered
/// under the same EXECUTABLE, with a DEPLOY, are refused, naming both, rather
/// than one of them being built into nothing.
#[test]
fn a_duplicate_executable_is_refused() -> TestResult<()> {
    let out = outcome("cmake_carrier_duplicate_executable_refused")?;
    assert!(
        !out.success(),
        "expected the configure to fail on a duplicate EXECUTABLE:\n{}",
        out.stderr
    );
    // CMake re-wraps a message at its own width.
    let err = out.stderr.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        err.contains("`talker` twice")
            && err.contains("`carrier_pkg::First`")
            && err.contains("`carrier_pkg::Second`")
            && err.contains("its own EXECUTABLE"),
        "expected the refusal to name the EXECUTABLE and both classes, got:\n{}",
        out.stderr
    );
    Ok(())
}

/// phase-583 W1 -- Zephyr builds ONE program per package (one west image), so
/// a second registration there is still refused, saying to split the package
/// and that several programs per Zephyr package are planned.
#[test]
fn a_second_zephyr_registration_is_refused() -> TestResult<()> {
    let out = outcome("cmake_carrier_zephyr_second_registration_refused")?;
    assert!(
        !out.success(),
        "expected the configure to fail on a second Zephyr registration:\n{}",
        out.stderr
    );
    let err = out.stderr.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        err.contains("`second_exe`")
            && err.contains("`first_exe`")
            && err.contains("ONE program per package")
            && err.contains("Split the package")
            && err.contains("planned"),
        "expected the refusal to name both and say to split the package, got:\n{}",
        out.stderr
    );
    Ok(())
}

/// phase-583 W1 -- target names are global. A program called `talker` in
/// ANOTHER package must not switch this package's `talker` carrier off; only
/// the package's own program of that name does.
#[test]
fn another_packages_program_does_not_switch_a_carrier_off() -> TestResult<()> {
    let out = outcome("cmake_carrier_other_package_program")?;
    assert!(
        out.success() && out.stdout.contains("carrier is claimed beside"),
        "another package's `talker` switched this carrier off:\n{}\n{}",
        out.stdout,
        out.stderr
    );
    Ok(())
}

/// Issue 2744 -- the negative control: a workspace member's second component
/// under the same EXECUTABLE (no DEPLOY) is composed by a generated entry, so
/// it builds no second carrier and is not refused.
#[test]
fn a_second_member_registration_without_deploy_is_not_refused() -> TestResult<()> {
    let out = outcome("cmake_carrier_member_without_deploy")?;
    assert!(
        out.success(),
        "a member's second component must configure:\n{}",
        out.stderr
    );
    Ok(())
}

/// Issue 2751 -- the host census build of an owned entry configures the
/// package alone: no `system.toml`, `NROS_LEAF_BOARD=linux`, and the entry
/// verb's host deploy in `NROS_DEPLOY`. Board facts answer from the stated
/// board instead of asking for an image `linux` no file declares.
#[test]
fn a_census_host_configure_resolves_board_facts_from_its_stated_board() -> TestResult<()> {
    let out = outcome("cmake_board_facts_census_host_board")?;
    assert!(
        out.success(),
        "the census host configure must resolve board facts:\n{}",
        out.stderr
    );
    Ok(())
}

/// Issue 2751 -- the precedence the fallback must keep: a named `NROS_BOARD`
/// outranks `NROS_LEAF_BOARD`. Naming a board no descriptor declares fails
/// the configure on THAT board, so the fallback never answers over it.
#[test]
fn a_named_board_outranks_the_census_leaf_board() -> TestResult<()> {
    let out = outcome("cmake_board_facts_board_outranks_leaf_board")?;
    assert!(
        !out.success(),
        "a named NROS_BOARD must be the board asked for:\n{}",
        out.stderr
    );
    assert!(
        out.stderr.contains("no_such_board_2751"),
        "the failure names the named board:\n{}",
        out.stderr
    );
    Ok(())
}

/// phase-583 W4 (RFC-0071 D20) -- an owned entry that checks ONE program keeps
/// the entry's stack-floor key `<pkg>@<board>` and the build dir's one report.
#[test]
fn one_checked_program_keeps_the_entry_stack_floor_key() -> TestResult<()> {
    let out = outcome("stack_floor_one_program_keeps_entry_key")?;
    assert!(
        out.success()
            && out.stdout.contains("VERDICT: key=carrier_pkg@b1")
            && out
                .stdout
                .contains("VERDICT: report=nros/stack-floor/report.json"),
        "one program must keep the entry's key and report:\n{}\n{}",
        out.stdout,
        out.stderr
    );
    Ok(())
}

/// phase-583 W4 -- an owned entry that checks two programs keys each program
/// `<pkg>/<exe>@<board>`, reporting to `nros/stack-floor/<exe>/report.json`.
#[test]
fn two_checked_programs_are_keyed_per_program() -> TestResult<()> {
    let out = outcome("stack_floor_two_programs_keyed_per_program")?;
    for x in ["prog_a", "prog_b"] {
        let want =
            format!("VERDICT: {x} key=carrier_pkg/{x}@b1 report=nros/stack-floor/{x}/report.json");
        assert!(
            out.success() && out.stdout.contains(&want),
            "expected `{want}`:\n{}\n{}",
            out.stdout,
            out.stderr
        );
    }
    Ok(())
}

/// phase-583 W4 -- a stated `stack_floor_image` cannot name two programs'
/// floors, so it is refused, naming both programs and what to remove.
#[test]
fn a_stated_stack_floor_key_with_two_programs_is_refused() -> TestResult<()> {
    let out = outcome("stack_floor_stated_key_with_two_programs_refused")?;
    assert!(
        !out.success(),
        "a stated key must be refused for two programs:\n{}",
        out.stdout
    );
    let err = out.stderr.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        err.contains("prog_a, prog_b") && err.contains("Remove `stack_floor_image`"),
        "{err}"
    );
    Ok(())
}

/// Issue 2746 -- a west build is one Zephyr image with one `main`. A second,
/// different program put into `app` is refused at configure, naming both
/// programs and saying to split the package, instead of failing at link with
/// `multiple definition of main`.
#[test]
fn a_second_zephyr_program_is_refused_at_configure() -> TestResult<()> {
    let out = outcome("cmake_zephyr_second_program_refused")?;
    assert!(
        !out.success(),
        "expected the configure to fail on a second Zephyr program:\n{}",
        out.stderr
    );
    let err = out.stderr.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        err.contains("`talker_b`")
            && err.contains("`talker`")
            && err.contains("one program per package")
            && err.contains("Split the package"),
        "expected the refusal to name both programs and say to split the package, got:\n{}",
        out.stderr
    );
    Ok(())
}

/// Issue 2746, the negative control: the same program reaching `app` twice is
/// one program, and the configure passes.
#[test]
fn one_zephyr_program_claimed_twice_passes() -> TestResult<()> {
    let out = outcome("cmake_zephyr_one_program_claimed_twice")?;
    assert!(
        out.success()
            && out
                .stdout
                .contains("one Zephyr program claimed twice passes"),
        "one program claimed twice was refused:\n{}\n{}",
        out.stdout,
        out.stderr
    );
    Ok(())
}
