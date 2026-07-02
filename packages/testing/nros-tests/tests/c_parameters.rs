//! C parameter-server integration test (phase-277 W5).
//!
//! Runs the prebuilt `c_parameters` example (the C sibling of
//! `cpp_parameters`, extracted from the pre-W5 `native/c/talker` demo
//! block), asserting that declare/get/set roundtrips through the C
//! `nros_param_server_t` API work end-to-end. The example exits with
//! status 0 only when every roundtrip passes — non-zero exit codes encode
//! which assertion failed (see `examples/native/c/parameters/src/main.c`).
//! Build it ahead of time with `just native build-fixtures`.

use std::process::Command;

use nros_tests::fixtures::{build_c_parameters, require_cmake};

#[test]
fn c_parameters_roundtrip() {
    assert!(
        require_cmake(),
        "c_parameters_roundtrip requires `cmake` on PATH"
    );

    let binary = build_c_parameters().expect("c-parameters fixture not prebuilt");

    let output = Command::new(binary)
        .output()
        .expect("failed to spawn c-parameters binary");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "c-parameters exited with {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        stdout,
        stderr,
    );

    // Declared defaults read back.
    assert!(
        stdout.contains("Parameters: verbose=false, rate=1 Hz, scale=1.00, topic=/chatter"),
        "expected declared-defaults line in stdout, got:\n{}",
        stdout,
    );
    // Set + get roundtrip.
    assert!(
        stdout.contains("After set: verbose=true"),
        "expected post-set line in stdout, got:\n{}",
        stdout,
    );
    assert!(
        stdout.contains("OK verbose=true rate=10 topic=/rosout"),
        "expected OK marker in stdout, got:\n{}",
        stdout,
    );
}
