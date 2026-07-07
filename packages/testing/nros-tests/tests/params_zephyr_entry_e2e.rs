//! phase-276 W1 (#102 H1, issue #128) — runtime E2E for PARAMETERS on embedded:
//! the `ws-params-rust` workspace's Zephyr (native_sim) Entry.
//!
//! Before #128 the `nros::main!` `Framework::Zephyr` emit arm carried only
//! register+spin — a `system.toml [param_services]` was silently ignored on
//! Zephyr. The fix gives the arm OwnedSpin parity: it now emits
//! `apply_param_services(&[launch-baked initials])` before the registers, so
//! the param store exists when the node's cell captures it and the six ROS 2
//! parameter services register over the session.
//!
//! Assertion mirrors the native `param_live_read_e2e`: the `param_talker`
//! node LIVE-reads its launch-baked `publish_period_ms` initial (`250`) via
//! `ctx.parameter::<i64>` in its callback and publishes that value; a separate
//! native nros subscriber must see `Received: 250` — proving the param was
//! baked, seeded into the (Zephyr-side) store, and read back on-target. The
//! `zephyr.exe` runs as a host process (native_sim, NSOS host sockets) and
//! dials the baked `tcp/127.0.0.1:17845` locator.
//!
//! The fixture is built by the west lane (`just zephyr build-fixtures`,
//! `--include-workspace-entry` params leaf); this test skips cleanly when
//! `zephyr.exe` is absent.
//!
//! Run with: `cargo nextest run -p nros-tests --test params_zephyr_entry_e2e`

use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, ZephyrPlatform, ZephyrProcess, build_int32_sink,
    build_zephyr_workspace_rust_params_entry, require_zenohd,
};
use std::{process::Command, time::Duration};

/// The router port baked into the params zephyr entry (the west lane's
/// `-DCONFIG_NROS_ZENOH_LOCATOR="tcp/127.0.0.1:17845"`).
const PARAMS_ZEPHYR_ENTRY_PORT: u16 = 17845;

#[test]
fn params_zephyr_entry_publishes_baked_initial() {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let entry = build_zephyr_workspace_rust_params_entry().unwrap_or_else(|e| {
        nros_tests::skip!("zephyr params workspace entry not built (west): {e}")
    });
    // #147/#278: the param_talker publishes std_msgs/Int32 on /chatter, so the
    // observer must be the typed Int32 sink (prints `Received: N`). The old
    // std_msgs/String `native_listener` only matched while its fixture was a
    // STALE pre-W4 Int32 build — a fresh (String) listener never emits
    // `Received: 250`. int32-sink defaults to /chatter.
    let listener = build_int32_sink()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("int32-sink fixture not built: {e}"));

    // Router on the exact port the fixture's CONFIG_NROS_ZENOH_LOCATOR was baked with.
    let router = ZenohRouter::start_on("127.0.0.1", PARAMS_ZEPHYR_ENTRY_PORT).unwrap_or_else(|e| {
        nros_tests::skip!("zenohd failed to start on {PARAMS_ZEPHYR_ENTRY_PORT}: {e}")
    });
    let observer_locator = router.locator();

    // Subscriber first, so its subscription is live before the native_sim talker publishes.
    let mut obs = {
        let mut cmd = Command::new(&listener);
        cmd.env("RUST_LOG", "info")
            .env("NROS_LOCATOR", &observer_locator)
            .env("NROS_SESSION_MODE", "client")
            .env("NROS_SUB_TOPIC", "/chatter");
        ManagedProcess::spawn_command(cmd, "int32-sink")
            .unwrap_or_else(|e| panic!("spawn listener: {e}"))
    };
    obs.wait_for_output_pattern("Waiting for Int32", Duration::from_secs(10))
        .unwrap_or_else(|_| {
            obs.kill();
            panic!("int32-sink never became ready")
        });

    // Boot the Zephyr native_sim image (runs until killed).
    let mut zephyr = ZephyrProcess::start(&entry, ZephyrPlatform::NativeSim)
        .unwrap_or_else(|e| panic!("boot zephyr native_sim: {e}"));

    // The published value IS the live param read: the baked initial is 250, so
    // seeing `Received: 250` cross-process proves the launch `<param>` was
    // compile-baked, seeded into the Zephyr entry's param store
    // (`apply_param_services`, the #128 emit), and live-read by the node's
    // callback on the embedded target.
    let out = obs
        .wait_for_output_count("Received: 250", 3, Duration::from_secs(90))
        .unwrap_or_else(|_| {
            zephyr.kill();
            obs.kill();
            panic!(
                "subscriber never saw the live-read baked param value (250) from the \
                 Zephyr native_sim params entry — params-on-embedded (276 W1 / #128) \
                 did not work"
            )
        });

    zephyr.kill();
    obs.kill();

    let n = nros_tests::count_pattern(&out, "Received: 250");
    assert!(n >= 3, "expected ≥3 live-read publishes of 250, got {n}");
}
