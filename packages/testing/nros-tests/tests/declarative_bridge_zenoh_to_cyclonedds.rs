//! phase-267 W-B — DECLARATIVE cross-RMW bridge e2e (zenoh → Cyclone DDS).
//!
//! The declarative sibling of `bridge_zenoh_to_cyclonedds.rs` (which drives the
//! hand-written `bridge-zenoh-to-cyclonedds-fwd` bin). Here the bridge is the
//! `ws-bridge-rust` `native_entry` — a PLAIN `nros::main!` whose
//! `demo_bringup/system.toml` declares a `[[bridge]]`. `nros sync` bakes
//! `nros-bridge.toml` (incl. the `std_msgs/Int32` field schema), the macro emits
//! `run_from_config_str` + the backend `register()` calls, and the runtime stages
//! the Cyclone descriptor from the config schema and forwards. This test codifies
//! the manual e2e that closed phase-267 W-B (issues 0106 / 0107 / 0109).
//!
//! ## Topology
//!
//! ```text
//!   native_rs_talker ── zenoh ──► zenohd ──► native_entry (declarative bridge)
//!   (rmw-zenoh fixture)           (router)        │  run_from_config pump
//!                                                 ▼
//!                                      nano cyclonedds C listener (domain 5)
//! ```
//!
//! ## Baked endpoints (and the concurrency caveat)
//!
//! Unlike the imperative bin (which takes `ZENOH_LOCATOR` / `ROS_DOMAIN_ID` from
//! the env), the declarative entry BAKES its endpoints from
//! `demo_bringup/system.toml` — `run_from_config` has no env override. So this
//! test pins zenohd to the baked locator and the listener to the baked cyclone
//! domain; both MUST mirror `system.toml`.
//!
//! Consequence: it cannot use `unique_ros_domain_id()` like the other cyclone
//! host tests, so there is a small residual risk that a CONCURRENT cyclone test
//! happens to draw the same baked domain (`5`) and cross-talks. The zenoh port is
//! safe — `ZenohRouter::start` frees the fixed port first, and no other test uses
//! it. The proper fix is env-overridable bridge endpoints (a follow-up); until
//! then this is accepted (gated test, low probability). Do NOT change the baked
//! domain without changing `system.toml`.
//!
//! Skips cleanly when `zenohd`, the bridge entry fixture (gated on the cyclonedds
//! submodule), or the nano cyclone listener fixture is not built.

use std::{
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use nros_tests::{
    count_pattern,
    fixtures::{
        ManagedProcess, Rmw, ZenohRouter, build_native_c_example_rmw,
        build_native_workspace_rust_bridge_entry, require_zenohd, talker_binary,
    },
};
use rstest::rstest;

/// Baked in `examples/workspaces/ws-bridge-rust/src/demo_bringup/system.toml`:
/// the zenoh session `locator` and the cyclonedds `[[domain]] dds` id. Keep in
/// sync with that file — the entry compiles them in.
const BAKED_ZENOH_PORT: u16 = 7447;
const BAKED_CYCLONE_DOMAIN: u8 = 5;

/// Resolve (building if needed) the native Cyclone C listener, or skip when the
/// fixtures aren't set up. Mirrors `bridge_zenoh_to_cyclonedds::nano_cyclone_listener`.
fn nano_cyclone_listener() -> PathBuf {
    build_native_c_example_rmw("listener", "c_listener", Rmw::Cyclonedds).unwrap_or_else(|e| {
        nros_tests::skip!(
            "native/c/listener cyclonedds fixture not built (run `just cyclonedds setup`): {e:?}"
        )
    })
}

fn spawn_cyclone_listener(binary: &Path, domain: u8) -> ManagedProcess {
    let mut cmd = Command::new(binary);
    cmd.env("ROS_DOMAIN_ID", domain.to_string())
        .env("RUST_LOG", "info");
    ManagedProcess::spawn_command(cmd, "nano-cyclone-listener-declarative-bridge")
        .expect("spawn nano cyclone listener")
}

fn spawn_zenoh_talker(bin: &Path, locator: &str) -> ManagedProcess {
    let mut cmd = Command::new(bin);
    cmd.env("RUST_LOG", "info").env("NROS_LOCATOR", locator);
    let mut talker = ManagedProcess::spawn_command(cmd, "native-rs-talker-declarative-bridge")
        .expect("spawn talker");
    talker
        .wait_for_output_pattern("Published", Duration::from_secs(8))
        .expect("talker did not publish first sample");
    talker
}

/// zenoh talker → `ws-bridge-rust` declarative bridge → nano cyclone listener.
/// Asserts the listener RECEIVES the forwarded `std_msgs/Int32` — the full
/// declarative path (descriptor staged from the config field schema, raw publish
/// accepted, sample delivered), no ROS 2 install needed.
#[rstest]
fn declarative_zenoh_to_cyclonedds_bridge_to_nano_listener(talker_binary: PathBuf) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    // Gated on the cyclonedds submodule — the entry build compiles vendored C++
    // Cyclone, so an unprovisioned tree leaves the fixture absent.
    let bridge_bin = match build_native_workspace_rust_bridge_entry() {
        Ok(p) => p.to_path_buf(),
        Err(e) => nros_tests::skip!(
            "ws-bridge-rust native_entry fixture not prebuilt ({e}); run \
             `just native build-workspace-fixtures` (needs `just cyclonedds setup`)"
        ),
    };
    let listener_bin = nano_cyclone_listener();

    // The entry bakes `tcp/127.0.0.1:7447`; pin zenohd to that port (the entry
    // connects to it as a client).
    let zenohd = ZenohRouter::start(BAKED_ZENOH_PORT).expect("start zenohd on the baked port");
    let zenoh_locator = zenohd.locator();

    // Listener first — its subscription must be discoverable before the bridge's
    // cyclone egress publisher matches over SPDP.
    let mut listener = spawn_cyclone_listener(&listener_bin, BAKED_CYCLONE_DOMAIN);
    std::thread::sleep(Duration::from_secs(3));

    // The declarative entry takes NO env (locator + domain are baked); it
    // connects to zenohd, opens the cyclone egress on domain 5, stages the
    // descriptor, and pumps. It has no startup banner, so give it a moment.
    let mut bridge_cmd = Command::new(&bridge_bin);
    bridge_cmd.env("RUST_LOG", "info");
    let mut bridge = ManagedProcess::spawn_command(bridge_cmd, "ws-bridge-rust-native_entry")
        .expect("spawn declarative bridge entry");
    std::thread::sleep(Duration::from_secs(2));

    let mut talker = spawn_zenoh_talker(&talker_binary, &zenoh_locator);

    let listener_output = listener
        .wait_for_output_count("Received", 2, Duration::from_secs(12))
        .unwrap_or_default();

    talker.kill();
    bridge.kill();
    listener.kill();
    drop(zenohd);

    eprintln!("nano cyclone listener output:\n{listener_output}");
    let received = count_pattern(&listener_output, "Received");
    eprintln!("nano cyclone listener received {received} bridged sample(s)");
    assert!(
        received >= 2,
        "expected ≥ 2 bridged samples to reach the nano cyclone listener \
         (zenoh → declarative ws-bridge-rust entry → cyclonedds), got {received}.\n\
         Full listener output:\n{listener_output}"
    );
}
