//! Issue #53 — zenoh → Cyclone DDS mixed-RMW bridge e2e (stock-cyclonedds
//! variant; the cyclonedds sibling of `bridge_mixed_rmw.rs`).
//!
//! ## Topology
//!
//! ```text
//!   native_rs_talker  ─── zenoh ───►  zenohd  ───► bridge-zenoh-to-cyclonedds-fwd
//!   (rmw-zenoh fixture)               (router)         │  in-process pump
//!                                                      ▼
//!                                              Cyclone DDS /chatter (RTPS)
//! ```
//!
//! ## What it asserts
//!
//! The bridge logs `forwarded <n> bytes zenoh→cyclonedds` for each sample whose
//! zenoh ingress callback fires AND whose Cyclone `publish_raw` returns `Ok` —
//! i.e. the cyclonedds-specific path (descriptor staged via
//! `nros_rmw::register_type_descriptor`, raw publisher created, `dds_write`
//! accepted) succeeds on **live** data crossing the RMW boundary. This is the
//! cyclonedds analogue of `bridge_mixed_rmw`'s XRCE round-trip; it needs no DDS
//! receiver (the bridge links the vendored CycloneDDS), so it runs with just
//! zenohd + the zenoh talker fixture.
//!
//! Skips cleanly when `zenohd` or the prebuilt bridge binary is missing — same
//! `require_*` pattern as the rest of the suite.

use std::{path::PathBuf, process::Command, time::Duration};

use nros_tests::{
    count_pattern,
    fixtures::{
        ManagedProcess, ZenohRouter, build_bridge_zenoh_to_cyclonedds_fwd, require_zenohd,
        talker_binary, zenohd_unique,
    },
};
use rstest::rstest;

#[rstest]
fn test_zenoh_to_cyclonedds_bridge_e2e(zenohd_unique: ZenohRouter, talker_binary: PathBuf) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let bridge_bin = match build_bridge_zenoh_to_cyclonedds_fwd() {
        Ok(p) => p.to_path_buf(),
        Err(e) => {
            nros_tests::skip!(
                "bridge-zenoh-to-cyclonedds-fwd binary not prebuilt ({e}); run `cargo build \
                 --profile nros-fast-release` inside \
                 packages/testing/nros-tests/bins/bridge-zenoh-to-cyclonedds-fwd/"
            );
        }
    };

    let zenoh_locator = zenohd_unique.locator();

    // 1. Spawn the bridge: zenoh ingress + Cyclone DDS egress on domain 0.
    let mut bridge_cmd = Command::new(&bridge_bin);
    bridge_cmd
        .env("RUST_LOG", "info")
        .env("ZENOH_LOCATOR", &zenoh_locator)
        .env("ROS_DOMAIN_ID", "0");
    let mut bridge = ManagedProcess::spawn_command(bridge_cmd, "bridge-zenoh-to-cyclonedds-fwd")
        .expect("spawn bridge");
    bridge
        .wait_for_output_pattern("Spinning", Duration::from_secs(10))
        .expect("bridge did not finish session setup (zenoh + cyclonedds egress)");

    // 2. Spawn the zenoh talker — its Int32 samples land on the bridge's zenoh
    //    ingress, which republishes them onto Cyclone DDS.
    let mut talker_cmd = Command::new(&talker_binary);
    talker_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &zenoh_locator);
    let mut talker = ManagedProcess::spawn_command(talker_cmd, "native-rs-talker-cyclonedds-bridge")
        .expect("spawn talker");
    talker
        .wait_for_output_pattern("Published", Duration::from_secs(8))
        .expect("talker did not publish first sample");

    // 3. Collect the bridge's forward log for a window covering ≥ 2 of the 1 Hz
    //    publishes (each ingress→egress hop is sub-millisecond).
    let bridge_output = bridge
        .wait_for_output_count("forwarded", 2, Duration::from_secs(10))
        .unwrap_or_default();

    talker.kill();
    bridge.kill();

    eprintln!("bridge output:\n{bridge_output}");
    let forwarded = count_pattern(&bridge_output, "forwarded");
    eprintln!("bridge forwarded {forwarded} sample(s) zenoh→cyclonedds");
    assert!(
        forwarded >= 2,
        "expected ≥ 2 samples forwarded zenoh→cyclonedds (descriptor staged + raw publish \
         accepted on live data), got {forwarded}.\nFull bridge output:\n{bridge_output}"
    );
}
