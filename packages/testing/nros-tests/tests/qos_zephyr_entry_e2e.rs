//! phase-276 W5 (#102 H1) — runtime E2E for QOS OVERRIDES on embedded: the
//! `ws-qos-rust` workspace's Zephyr (native_sim) Entry.
//!
//! The QoS profiles are declared per-entity in NODE CODE (RFC-0041):
//! `reliable_talker` publishes `/qos_chatter` with a NON-DEFAULT profile
//! (reliable + transient_local) and `qos_listener` subscribes with the
//! byte-identical profile — matching the profile is the per-entity QoS
//! override capability. Both nodes run ON-TARGET inside the Zephyr image and
//! share ONE zenoh session (RFC-0015 Model 1), so the in-image delivery leg
//! rides `Z_FEATURE_LOCAL_SUBSCRIBER` (neither zenoh-pico nor the router
//! loops a publication back to its own session). The listener republishes
//! its matched receive count on `/qos_ok`.
//!
//! Assertion mirrors `params_zephyr_entry_e2e`: the native `int32-sink`
//! fixture subscribes `/qos_ok` over the same router and must log `Received:`
//! — proving the on-target non-default-QoS pair matched, delivered in-image,
//! and the republish reached the wire. (Not asserted via `ros2 topic echo`:
//! the zephyr↔rmw_zenoh_cpp data interop is a separate, unproven axis.)
//!
//! Requires the west-lane fixture (`just zephyr build-fixtures`; skips when
//! `zephyr.exe` is absent) and the `int32-sink` fixture binary.
//!
//! Run with: `cargo nextest run -p nros-tests --test qos_zephyr_entry_e2e`

use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, ZephyrPlatform, ZephyrProcess, build_int32_sink,
    build_zephyr_workspace_rust_qos_entry,
};
use std::{process::Command, time::Duration};

/// The router port baked into the qos zephyr entry (the west lane's
/// `-DCONFIG_NROS_ZENOH_LOCATOR="tcp/127.0.0.1:17849"`).
const QOS_ZEPHYR_ENTRY_PORT: u16 = 17849;

#[test]
fn qos_zephyr_entry_matched_pair_delivers() {
    let entry = build_zephyr_workspace_rust_qos_entry()
        .unwrap_or_else(|e| nros_tests::skip!("zephyr qos workspace entry not built (west): {e}"));
    let observer = build_int32_sink()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("int32-sink fixture not built: {e}"));

    // Router on the exact port the fixture's CONFIG_NROS_ZENOH_LOCATOR was baked with.
    let router = ZenohRouter::start_on("127.0.0.1", QOS_ZEPHYR_ENTRY_PORT).unwrap_or_else(|e| {
        nros_tests::skip!("zenohd failed to start on {QOS_ZEPHYR_ENTRY_PORT}: {e}")
    });
    let locator = router.locator();

    // Observer first, so its subscription is live before the image republishes.
    let mut obs = {
        let mut cmd = Command::new(&observer);
        cmd.env("RUST_LOG", "info")
            .env("NROS_LOCATOR", &locator)
            .env("NROS_SESSION_MODE", "client")
            .env("NROS_SUB_TOPIC", "/qos_ok");
        ManagedProcess::spawn_command(cmd, "int32-sink")
            .unwrap_or_else(|e| panic!("spawn observer: {e}"))
    };
    obs.wait_for_output_pattern("Waiting for Int32", Duration::from_secs(10))
        .unwrap_or_else(|_| {
            obs.kill();
            panic!("int32-sink never became ready")
        });

    // Boot the Zephyr native_sim image (runs until killed).
    let mut zephyr = ZephyrProcess::start(&entry, ZephyrPlatform::NativeSim)
        .unwrap_or_else(|e| panic!("boot zephyr native_sim: {e}"));

    // `/qos_ok` carries the listener's running receive count — samples there
    // mean the on-target reliable+transient_local pair matched and delivered.
    let _ = obs
        .wait_for_output_count("Received:", 3, Duration::from_secs(90))
        .unwrap_or_else(|_| {
            zephyr.kill();
            obs.kill();
            panic!(
                "observer never saw 3 `/qos_ok` republishes from the Zephyr entry — \
                 the on-target non-default-QoS (reliable+transient_local) pair did \
                 not match/deliver (276 W5)"
            )
        });

    zephyr.kill();
    obs.kill();
}
