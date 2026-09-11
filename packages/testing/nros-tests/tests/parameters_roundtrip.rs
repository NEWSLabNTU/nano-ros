//! Parameter declare/get/set roundtrips through the C and C++ APIs.
//!
//! phase-373 W4 — the fold of `c_parameters.rs` (phase-277 W5) and
//! `cpp_parameters.rs` (phase-117.9). The two files were the same test with two
//! nouns swapped: spawn a prebuilt example, require exit 0, then grep stdout.
//! Only the builder and the expected lines differed, so the runner is shared and
//! the lines are the case data.
//!
//! phase-426 W4 — the examples moved onto THE parameter store. They used to
//! exercise a caller-owned second store (`nros::ParameterServer<8>` in C++, an
//! `nros_parameter_server_t` over a static array in C), whose contents the six
//! `rcl_interfaces/srv/*` servers could not read — so both shipped the exact
//! defect this phase exists to remove. They are ROS nodes now, which is why
//! this test needs a router: parameters belong to a node, a node needs a
//! session, and a session needs something to talk to.
//!
//! The examples are the real assertion. Each exits 0 only when every roundtrip
//! passes, and encodes WHICH assertion failed in its non-zero exit code — see
//! `examples/native/{c/parameters/src/main.c, cpp/parameters/src/main.cpp}`. The
//! stdout greps below pin the values actually read back, so an example that
//! exits 0 while printing defaults still fails here.
//!
//! What this test does NOT cover is the wire half — that `ros2 param list
//! /cpp_parameters` enumerates them. That needs a ROS 2 peer and lives in the
//! interop lane; the examples' own file comments carry the commands.
//!
//! Build ahead of time with `just native build-fixtures`; nothing compiles at
//! run time.

use nros_tests::{
    TestResult,
    fixtures::{
        ZenohRouter, build_c_parameters, build_cpp_parameters, require_cmake, require_zenohd,
        zenohd_unique,
    },
};
use rstest::rstest;
use std::{path::Path, process::Command};

/// The C example: declared defaults, then a set, then the OK marker.
const C_EXPECTED: &[&str] = &[
    "Parameters: verbose=false, rate=1 Hz, scale=1.00, topic=/chatter",
    "After set: verbose=true",
    "OK verbose=true rate=10 topic=/rosout",
];

/// The C++ example: scalars through `rclcpp::Node::declare_parameter<T>`, then
/// the `nros::Seq<double, 8>` sequence parameter (declared with three elements,
/// updated to four, read back).
const CPP_EXPECTED: &[&str] = &[
    "Parameters: verbose=false, max_iters=100, ctrl_period=0.150000, frame_id=base_link",
    "After set: ctrl_period=0.050000 frame_id=map",
    "OK mpc_weights[0]=4.000000 n=4",
];

#[rstest]
#[case::c("c", build_c_parameters as fn() -> nros_tests::TestResult<&'static Path>, C_EXPECTED)]
#[case::cpp("cpp", build_cpp_parameters as fn() -> nros_tests::TestResult<&'static Path>, CPP_EXPECTED)]
fn parameters_roundtrip(
    #[case] lang: &str,
    #[case] build: fn() -> TestResult<&'static Path>,
    #[case] expected: &[&str],
    zenohd_unique: ZenohRouter,
) {
    assert!(
        require_cmake(),
        "{lang}_parameters_roundtrip requires `cmake` on PATH"
    );
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let binary =
        build().unwrap_or_else(|e| panic!("{lang}-parameters fixture not prebuilt: {e:?}"));

    let output = Command::new(binary)
        .env("NROS_LOCATOR", zenohd_unique.locator())
        .env("NROS_SESSION_MODE", "client")
        // The roundtrip is synchronous; the spin afterwards only exists so a
        // human can point `ros2 param get` at the node. Skip it here.
        .env("NROS_SPIN_MS", "0")
        .env(
            "ROS_DOMAIN_ID",
            nros_tests::unique_ros_domain_id().to_string(),
        )
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
