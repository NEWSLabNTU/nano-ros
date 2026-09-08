//! Emitted ROS message code compiles clean — asserted from the BUILD stage
//! (issue 1230; the AGENTS.md "No compilation inside tests" rule, issue 0034).
//!
//! ## What moved
//!
//! `packages/cli/rosidl-codegen/tests/compilation_test.rs` had four tests that
//! generated a message crate with `generate_message_package(…)` into a
//! `tempfile` tree and then spawned `cargo check` (×3) or `cargo clippy` (×1) on
//! it, at TEST time. They are now one `[[compile_check_fixture]]` row,
//! `generated_message_crate`, builder `cargo-clippy`: `build.rs` in
//! `fixtures/generated_message_crate/` runs the same emitter over the same four
//! `.msg` inputs — which were string CONSTANTS in the test bodies, never
//! anything a running test computed — and the build stage compiles the result
//! under `#![deny(warnings)]` + `#![deny(clippy::all)]`.
//!
//! This test asserts the stamp. Same shape as `platform_header_compile.rs`,
//! whose nine positive cells moved the same way in phase-329 W5.
//!
//! ## What that buys, beyond obeying the rule
//!
//! * The compile is shared and cached. `cargo test -p rosidl-codegen` paid
//!   10.7 s for 267 tests, **9.9 s of it in `test_check_no_warnings` alone**;
//!   `check-cli-tests` is a required check on every pull request and paid that
//!   per push. The fixture costs ~36 s cold, once per `build-test-fixtures`,
//!   in one row of a jobserver-parallel lane.
//! * A crate-level `deny` reaches ALL FOUR messages. Each old test generated
//!   ONE message and carried its own attributes, so `test_check_no_warnings`
//!   asserted "no warnings" over `Point` (`int32`/`float64`) — the one shape
//!   with no String field, and therefore the one that could not produce the
//!   `invalid_value` warning its two String-carrying siblings did produce.
//! * Clippy's verdict is now a verdict. `test_clippy_no_warnings` ran
//!   `-W clippy::all` and failed only if clippy's stderr contained the
//!   substring `"error"` — i.e. every clippy WARNING, which is all that
//!   invocation could emit, passed. Denying the lints in the crate under check
//!   is what makes a red possible; the first thing it found is issue 1244.
//!
//! What did NOT move: the emitted code is compiled against the same
//! hand-written `rosidl_runtime_rs` stub the tests used, so this proves the
//! emitter's output is well-formed Rust against that shape — not that it works
//! against the real ros2_rust runtime, which the tests never claimed either.

use nros_tests::TestResult;
use std::path::PathBuf;

/// The fixture id built by `scripts/build/compile-check-fixtures.sh`.
const FIXTURE_ID: &str = "generated_message_crate";

/// The message shapes the fixture must carry, one `.msg` each.
///
/// A plain list, like `platform_header_compile.rs`'s snippet ids — not a matrix
/// axis. It exists because deleting an input is otherwise invisible: `build.rs`
/// refuses an EMPTY `msgs/`, but three files still build clean and would
/// silently retire whichever intent the fourth carried (arrays under serde is
/// the one with no sibling anywhere else in the tree).
const REQUIRED_MESSAGES: &[&str] = &["SimpleMsg", "ArrayMsg", "Point", "TestMsg"];

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/generated_message_crate")
}

/// The generated message crate compiled clean at the build stage.
///
/// A red here is one of two things, and the stamp resolver tells them apart:
/// `.build-failed` beside the missing stamp means the emitted code stopped
/// compiling or stopped passing its lints (the regression this exists to
/// catch); no stamp and no marker means nobody built the fixture
/// (`just build-test-fixtures`).
#[test]
fn generated_message_code_compiles_and_lints_clean() -> TestResult<()> {
    let stamp = nros_tests::fixtures::require_compile_check(FIXTURE_ID)?;
    assert!(
        stamp.exists(),
        "compile-ok stamp missing for `{FIXTURE_ID}`: {}",
        stamp.display()
    );
    Ok(())
}

/// Every message shape the four replaced tests covered is still an input.
#[test]
fn every_covered_message_shape_is_still_an_input() {
    let msgs = fixture_dir().join("msgs");
    for name in REQUIRED_MESSAGES {
        let path = msgs.join(format!("{name}.msg"));
        assert!(
            path.is_file(),
            "`{name}.msg` is gone from {}. It is one of the four message shapes \
             issue 1230 moved out of `compilation_test.rs`; dropping it removes \
             coverage without failing the build.",
            msgs.display()
        );
    }
}
