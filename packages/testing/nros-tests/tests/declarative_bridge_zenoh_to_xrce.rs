//! phase-267 (xrce variant) — DECLARATIVE cross-RMW bridge e2e (zenoh → XRCE).
//!
//! The declarative sibling of `bridge_mixed_rmw::test_zenoh_to_xrce_bridge_e2e`
//! (which drives the hand-written `bridge-zenoh-to-xrce-fwd` bin). Here the bridge
//! is the `ws-bridge-xrce-rust` `native_entry` — a PLAIN `nros::main!` whose
//! `system.toml` declares a `[[bridge]]` to `xrce:agent`. `nros sync` bakes
//! `nros-bridge.toml` (the xrce egress node carries the agent locator), the macro
//! emits `run_from_config_str` + the backend `register()` calls, and the runtime
//! forwards. xrce is LAZY-registration, so there is NO descriptor staging.
//!
//! ## Topology
//!
//! ```text
//!   native_rs_talker ─ zenoh ─► zenohd ─► native_entry (xrce bridge) ─ XRCE ─►
//!                                                                   MicroXRCEAgent
//!                                                                        │ DDS
//!                                                                        ▼
//!                                                          nros xrce listener
//! ```
//!
//! The entry BAKES its endpoints; phase-267 #113 overrides them at runtime so the
//! test uses an ephemeral zenohd + a unique agent + a unique cyclone domain
//! (`NROS_BRIDGE_S0_LOCATOR` / `NROS_BRIDGE_S1_LOCATOR` / `NROS_BRIDGE_S1_DOMAIN`).
//! The agent's DDS participant joins the XRCE-client-requested domain, so the
//! bridge egress and the listener share it.
//!
//! Skips cleanly when `zenohd`, the XRCE Agent, the bridge entry fixture, or the
//! xrce listener fixture is not built.

use std::{path::PathBuf, process::Command, time::Duration};

use nros_tests::{
    count_pattern,
    fixtures::{
        ManagedProcess, XrceAgent, ZenohRouter, build_native_workspace_rust_bridge_xrce_entry,
        require_xrce_agent, require_zenohd, talker_binary, xrce_listener_binary, zenohd_unique,
    },
};
use rstest::rstest;

/// The generated `nros-bridge.toml` interns sessions to `s0` (zenoh ingress) and
/// `s1` (xrce egress); #113 overrides each at runtime.
const ZENOH_NODE: &str = "S0";
const XRCE_NODE: &str = "S1";

#[rstest]
fn declarative_zenoh_to_xrce_bridge_to_nros_listener(
    zenohd_unique: ZenohRouter,
    talker_binary: PathBuf,
    xrce_listener_binary: PathBuf,
) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    if !require_xrce_agent() {
        nros_tests::skip!("XRCE-DDS Agent not found");
    }
    let bridge_bin = match build_native_workspace_rust_bridge_xrce_entry() {
        Ok(p) => p.to_path_buf(),
        Err(e) => nros_tests::skip!(
            "ws-bridge-xrce-rust native_entry fixture not prebuilt ({e}); run \
             `just native build-workspace-fixtures`"
        ),
    };

    let zenoh_locator = zenohd_unique.locator();
    let agent = XrceAgent::start_unique().expect("XRCE Agent start");
    let xrce_locator = agent.addr().to_string();
    let domain = nros_tests::unique_ros_domain_id();
    // The agent's accept loop needs ~1 s to settle before clients connect (else
    // the first XRCE connect hits Transport(ConnectionFailed)).
    std::thread::sleep(Duration::from_secs(1));

    // Bridge first — its egress XRCE publisher is what the listener discovers.
    // #113 overrides: zenoh ingress → the ephemeral router, xrce egress → the
    // unique agent, on the unique domain. No "Spinning" banner (plain nros::main!),
    // so give it a moment.
    let mut bridge_cmd = Command::new(&bridge_bin);
    bridge_cmd
        .env("RUST_LOG", "info")
        .env(format!("NROS_BRIDGE_{ZENOH_NODE}_LOCATOR"), &zenoh_locator)
        .env(
            format!("NROS_BRIDGE_{XRCE_NODE}_LOCATOR"),
            format!("udp/{xrce_locator}"),
        )
        .env(
            format!("NROS_BRIDGE_{XRCE_NODE}_DOMAIN"),
            domain.to_string(),
        );
    let mut bridge = ManagedProcess::spawn_command(bridge_cmd, "ws-bridge-xrce-rust-native_entry")
        .expect("spawn declarative xrce bridge entry");
    std::thread::sleep(Duration::from_secs(2));

    // xrce listener — connects to the same agent on the same domain.
    let mut listener_cmd = Command::new(&xrce_listener_binary);
    listener_cmd
        .env("RUST_LOG", "info")
        .env("ROS_DOMAIN_ID", domain.to_string())
        .env("NROS_LOCATOR", format!("udp/{xrce_locator}"));
    let mut listener =
        ManagedProcess::spawn_command(listener_cmd, "xrce-listener-declarative-bridge")
            .expect("spawn xrce listener");
    listener
        .wait_for_output_pattern("Waiting for", Duration::from_secs(8))
        .expect("xrce listener did not become ready");

    let mut talker_cmd = Command::new(&talker_binary);
    talker_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &zenoh_locator);
    let mut talker = ManagedProcess::spawn_command(talker_cmd, "native-rs-talker-xrce-bridge")
        .expect("spawn talker");
    talker
        .wait_for_output_pattern(
            nros_tests::output::TALKER_LOG_PREFIX,
            Duration::from_secs(8),
        )
        .expect("talker did not publish first sample");

    let listener_output = listener
        .wait_for_output_count(
            nros_tests::output::LISTENER_LOG_PREFIX,
            2,
            Duration::from_secs(10),
        )
        .unwrap_or_default();

    talker.kill();
    bridge.kill();
    listener.kill();
    drop(agent);

    eprintln!("xrce listener output:\n{listener_output}");
    let received = count_pattern(&listener_output, nros_tests::output::LISTENER_LOG_PREFIX);
    eprintln!("xrce listener received {received} bridged sample(s)");
    assert!(
        received >= 2,
        "expected ≥ 2 bridged samples to reach the xrce listener \
         (zenoh → declarative ws-bridge-xrce-rust entry → xrce agent), got {received}.\n\
         Full listener output:\n{listener_output}"
    );
}
