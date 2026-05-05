//! C++ parameter wrapper integration test (Phase 117.9).
//!
//! Builds and runs the `cpp_parameters` example, asserting that
//! declare/get/set roundtrips through `nros::ParameterServer<Cap>` work
//! end-to-end. The example exits with status 0 only when every
//! roundtrip passes — non-zero exit codes encode which assertion
//! failed (see `examples/native/cpp/zenoh/parameters/src/main.cpp`).

use std::process::Command;

use nros_tests::fixtures::{build_cpp_parameters, require_cmake};

#[test]
fn cpp_parameters_roundtrip() {
    assert!(
        require_cmake(),
        "cpp_parameters_roundtrip requires `cmake` on PATH"
    );

    let binary = build_cpp_parameters().expect("failed to build cpp-parameters example");

    let output = Command::new(binary)
        .output()
        .expect("failed to spawn cpp-parameters binary");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "cpp-parameters exited with {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        stdout,
        stderr,
    );

    assert!(
        stdout.contains("OK use_sim_time=1"),
        "expected OK marker in stdout, got:\n{}",
        stdout,
    );
    assert!(
        stdout.contains("ctrl_period=0.050000"),
        "expected updated ctrl_period in stdout, got:\n{}",
        stdout,
    );
    assert!(
        stdout.contains("frame_id=map"),
        "expected updated frame_id in stdout, got:\n{}",
        stdout,
    );
}
