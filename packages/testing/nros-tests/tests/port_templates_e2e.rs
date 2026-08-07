//! Phase 209 C++ port templates — the acceptance, executed.
//!
//! **Bucket: KEEP — behavior one-off, not a matrix cell.** `matrix::CELLS`
//! coordinates are (platform × lang × rmw × workload); what these templates
//! prove is not a delivery workload but a PORTING property: a stock ROS 2 C++
//! node, vendored verbatim, builds and runs against nano-ros with only build
//! glue changed. There is no cell axis for "unmodified upstream source".
//!
//! # Why this file exists
//!
//! Issue 0469. Phase 209's three port templates were in NO lane — no fixture
//! row, no test, no recipe — for over two months. Nothing built or ran them
//! between 2026-05-30 and 2026-08-07, and in that window the acceptance
//! silently stopped holding: issue 0465, the rclcpp shim opening a second RMW
//! session, so the node died at startup with `Transport(ConnectionFailed)`.
//!
//! The shape of that failure decides the shape of this test. The template
//! **compiled and linked cleanly the entire time it was broken** — so a
//! build-only fixture row would have stayed green throughout and taught us
//! nothing. The acceptance is "compiles + links + RUNS"; only the third part
//! was lost, so the third part is what must be asserted here.
//!
//! The binaries come from `examples/fixtures.toml`
//! (`cpp_port_*`, builder `cmake-configure`) — tests never compile
//! (AGENTS.md Testing).

use nros_tests::{
    fixtures::{ManagedProcess, ZenohRouter, require_zenohd, zenohd_unique},
    output::CPP_PORT_PUBLISH_MARKER,
};
use rstest::rstest;
use std::time::Duration;

/// The canonical ROS 2 "minimal publisher", vendored verbatim, publishes over
/// nano-ros.
///
/// This is phase 209's headline claim, and the one that rotted. Asserting the
/// marker rather than merely "the process stayed up" matters: under issue 0465
/// the process also exited, but a shim that opens a session and then publishes
/// nothing would satisfy a liveness check while failing the actual promise.
#[rstest]
fn cpp_port_minimal_publisher_publishes(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let bin = nros_tests::fixtures::require_cmake_fixture(
        "cpp_port_minimal_publisher",
        "minimal_publisher",
    )
    .expect("phase-209 port template fixture");

    let mut cmd = std::process::Command::new(bin);
    cmd.env("NROS_LOCATOR", zenohd_unique.locator());
    let mut node = ManagedProcess::spawn_command(cmd, "cpp-port-minimal-publisher")
        .expect("spawn the ported node");

    // The template logs through the rclcpp compat surface's `RCLCPP_INFO`, so a
    // failure here is either "it never got a session" (0465's shape) or "the log
    // macro lost the line" — both worth failing on.
    // `wait_for_output_pattern` returns `Ok(output)` on TIMEOUT too, as long as
    // the process printed anything at all — it is "collect output, stopping
    // early if the pattern shows up", not an assertion. Checking only the
    // `Result` is how this test first passed against a deliberately broken
    // fixture: the failing node's `Transport(InvalidConfig)` line is non-empty
    // output, so the call returned `Ok`. Assert on the CONTENT.
    let out = node.collect_until(CPP_PORT_PUBLISH_MARKER, Duration::from_secs(20));
    assert!(
        out.contains(CPP_PORT_PUBLISH_MARKER),
        "the vendored ROS 2 tutorial node did not publish through nano-ros \
         (expected a line containing `{CPP_PORT_PUBLISH_MARKER}`).\n\
         Phase 209's acceptance is that this source builds AND RUNS unmodified; \
         issue 0465 was exactly this symptom, from the rclcpp shim opening a \
         second RMW session on a one-entry pool.\n\
         --- node output ---\n{out}"
    );
}
