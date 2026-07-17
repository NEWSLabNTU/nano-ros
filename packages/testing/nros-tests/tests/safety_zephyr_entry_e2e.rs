//! phase-276 W4 (#102 H1) — runtime E2E for E2E-SAFETY (CRC) on embedded: the
//! `ws-safety-rust` workspace's Zephyr (native_sim) Entry.
//!
//! The system declares `[system].features = ["safety"]` (phase-261 W4), which
//! lowers to the Rust `safety-e2e` backend feature: the zenoh backend attaches
//! the E2E CRC + sequence number on publish and validates on receive
//! (RFC-0028). Both nodes run ON-TARGET inside the Zephyr image and share ONE
//! zenoh session (RFC-0015 Model 1; the in-image delivery leg rides
//! `Z_FEATURE_LOCAL_SUBSCRIBER`). `safe_listener` reads the per-message
//! `CallbackCtx::integrity()` and republishes its CRC-VALIDATED receive count
//! on `/safe_ok` — the count climbs only while integrity is valid.
//!
//! Assertion mirrors `qos_zephyr_entry_e2e`: the native `int32-sink` fixture
//! subscribes `/safe_ok` over the same router and must log `Received:` —
//! proving the CRC attach → in-image deliver → validate → republish chain ran
//! on the embedded target.
//!
//! Requires the west-lane fixture (`just zephyr build-fixtures`; skips when
//! `zephyr.exe` is absent) and the `int32-sink` fixture binary.
//!
//! Run with: `cargo nextest run -p nros-tests --test safety_zephyr_entry_e2e`

use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, ZephyrPlatform, ZephyrProcess, build_int32_sink,
    build_zephyr_workspace_rust_safety_entry,
};
use std::{process::Command, time::Duration};

/// The router port baked into the safety zephyr entry (the west lane's
/// `-DCONFIG_NROS_ZENOH_LOCATOR="tcp/127.0.0.1:17851"`).
const SAFETY_ZEPHYR_ENTRY_PORT: u16 = 17851;

#[test]
fn safety_zephyr_entry_crc_validated_delivers() {
    let entry = build_zephyr_workspace_rust_safety_entry().unwrap_or_else(|e| {
        nros_tests::skip!("zephyr safety workspace entry not built (west): {e}")
    });
    let observer = build_int32_sink()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("int32-sink fixture not built: {e}"));

    // Router on the exact port the fixture's CONFIG_NROS_ZENOH_LOCATOR was baked with.
    let router = ZenohRouter::start_on("127.0.0.1", SAFETY_ZEPHYR_ENTRY_PORT).unwrap_or_else(|e| {
        nros_tests::skip!("zenohd failed to start on {SAFETY_ZEPHYR_ENTRY_PORT}: {e}")
    });
    let locator = router.locator();

    // Observer first, so its subscription is live before the image republishes.
    let mut obs = {
        let mut cmd = Command::new(&observer);
        cmd.env("RUST_LOG", "info")
            .env("NROS_LOCATOR", &locator)
            .env("NROS_SESSION_MODE", "client")
            .env("NROS_SUB_TOPIC", "/safe_ok");
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

    // `/safe_ok` carries the CRC-VALIDATED receive count — samples there mean
    // the on-target CRC attach → deliver → validate chain ran end-to-end.
    let _ = obs
        .wait_for_output_count(
            nros_tests::output::INT32_LISTENER_LOG_PREFIX,
            3,
            Duration::from_secs(90),
        )
        .unwrap_or_else(|_| {
            zephyr.kill();
            obs.kill();
            panic!(
                "observer never saw 3 `/safe_ok` republishes from the Zephyr entry — \
                 the on-target E2E-safety (CRC) path did not validate/deliver (276 W4)"
            )
        });

    zephyr.kill();
    obs.kill();
}
