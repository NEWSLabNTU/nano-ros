//! PX4 XRCE-DDS companion tests — Phase 233.4 (RFC-0039 Track B).
//!
//! Exercises the `px4_msgs` round-trip over a real `MicroXRCEAgent`. The
//! `px4-stub` example, in loopback mode, publishes `VehicleOdometry` on
//! `/fmu/out/vehicle_odometry` and subscribes its own topic in the same XRCE
//! session — a single-session pub+sub matches intra-participant, so the full
//! serialize → agent → deserialize round-trip flows against a *bare* agent.
//!
//! The companion ↔ PX4 cross-session receive is NOT covered here: it hits an
//! `nros-rmw-xrce` bug where a session holding both a publisher and a
//! subscriber starves the subscriber (independent of agent/type) — see
//! `docs/issues/0026-px4-xrce-bare-agent-type-matching.md`. This loopback test
//! uses pub+sub on the **same** topic, which matches intra-participant and so
//! sidesteps that bug while still exercising the px4_msgs CDR + QoS + XRCE path.
//!
//! Prerequisites:
//!   just build-xrce-agent   # or `nros setup` provisions MicroXRCEAgent
//!   just px4 build-fixtures  # builds the px4-stub binary to target-xrce/

use std::{path::PathBuf, process::Command, time::Duration};

use nros_tests::fixtures::{ManagedProcess, XrceAgent, px4_stub_binary, require_xrce_agent};
use rstest::rstest;

/// The px4-stub in loopback mode round-trips `px4_msgs/VehicleOdometry`
/// through the agent and receives its own samples back.
#[rstest]
fn test_px4_msgs_roundtrip_over_agent(px4_stub_binary: PathBuf) {
    if !require_xrce_agent() {
        nros_tests::skip!("XRCE agent not available");
    }

    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();
    let domain = nros_tests::unique_ros_domain_id().to_string();

    let mut cmd = Command::new(&px4_stub_binary);
    cmd.env("NROS_LOCATOR", &addr)
        .env("ROS_DOMAIN_ID", &domain)
        .env("PX4_STUB_LOOPBACK", "1")
        .env("PX4_STUB_TICKS", "200")
        .env("RUST_LOG", "info");
    let mut stub =
        ManagedProcess::spawn_command(cmd, "px4-stub-loopback").expect("Failed to start px4-stub");

    // Require several round-tripped samples — proves the px4_msgs CDR + the
    // px4() QoS profile + the XRCE pub/sub path work end-to-end over the agent.
    stub.wait_for_output_count("loopback rx[", 5, Duration::from_secs(30))
        .expect("px4-stub did not round-trip 5 VehicleOdometry samples through the agent");
}
