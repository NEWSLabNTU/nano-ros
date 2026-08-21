//! Parameter declare/get/set roundtrips through the C and C++ APIs.
//!
//! phase-373 W4 — the fold of `c_parameters.rs` (phase-277 W5) and
//! `cpp_parameters.rs` (phase-117.9). The two files were the same test with two
//! nouns swapped: spawn a prebuilt example, require exit 0, then grep stdout.
//! Only the builder and the expected lines differed, so the runner is shared and
//! the lines are the case data.
//!
//! The examples are the real assertion. Each exits 0 only when every roundtrip
//! passes, and encodes WHICH assertion failed in its non-zero exit code — see
//! `examples/native/{c/parameters/src/main.c, cpp/parameters/src/main.cpp}`. The
//! stdout greps below pin the values actually read back, so an example that
//! exits 0 while printing defaults still fails here.
//!
//! Build ahead of time with `just native build-fixtures`; nothing compiles at
//! run time.

use nros_tests::{
    TestResult,
    fixtures::{build_c_parameters, build_cpp_parameters, require_cmake},
};
use rstest::rstest;
use std::{path::Path, process::Command};

/// The C example: declared defaults, then a set, then the OK marker.
const C_EXPECTED: &[&str] = &[
    "Parameters: verbose=false, rate=1 Hz, scale=1.00, topic=/chatter",
    "After set: verbose=true",
    "OK verbose=true rate=10 topic=/rosout",
];

/// The C++ example: `nros::ParameterServer<Cap>` roundtrips, including the
/// phase-242.3 sequence parameter (declared, updated to 4 elements, read back).
const CPP_EXPECTED: &[&str] = &[
    "OK use_sim_time=1",
    "ctrl_period=0.050000",
    "frame_id=map",
    "mpc_weights[0]=4.000000 n=4",
];

#[rstest]
#[case::c("c", build_c_parameters as fn() -> nros_tests::TestResult<&'static Path>, C_EXPECTED)]
#[case::cpp("cpp", build_cpp_parameters as fn() -> nros_tests::TestResult<&'static Path>, CPP_EXPECTED)]
fn parameters_roundtrip(
    #[case] lang: &str,
    #[case] build: fn() -> TestResult<&'static Path>,
    #[case] expected: &[&str],
) {
    assert!(
        require_cmake(),
        "{lang}_parameters_roundtrip requires `cmake` on PATH"
    );

    let binary =
        build().unwrap_or_else(|e| panic!("{lang}-parameters fixture not prebuilt: {e:?}"));

    let output = Command::new(binary)
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn {lang}-parameters binary: {e}"));

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "{lang}-parameters exited with {:?}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        output.status.code(),
    );

    for line in expected {
        assert!(
            stdout.contains(line),
            "{lang}-parameters: expected `{line}` in stdout, got:\n{stdout}"
        );
    }
}
