//! Phase 269 W3 — runtime E2E for the C/C++ component subscription integrity API,
//! cross-process (two nros processes, no ROS 2 required).
//!
//! In-process delivery does not happen (same-session subscriber never receives a
//! same-session publisher; issue 0096), so each workspace is split into two entries:
//!
//!   talker entry     → boots the talker node; publishes /chatter with CRC attached
//!                       (automatic when built with NANO_ROS_SAFETY_E2E=ON, i.e.
//!                       `[system].features = ["safety"]` in system.toml).
//!   listener entry   → boots the listener node; the subscription validates the CRC,
//!                       increments a counter for each CRC-valid frame, and republishes
//!                       the count on /safe_ok (std_msgs/Int32).
//!
//! An external nros subscriber on /safe_ok asserts the count climbs, proving the
//! validated-callback path end-to-end: talker publish → backend CRC attach →
//! network transit → runtime validate → C/C++ component callback with integrity scalars →
//! republish.
//!
//! Two test functions:
//!   - `c_component_subscription_validated_delivers_crc_valid_count`
//!     C talker + C listener (`nros_cpp_subscription_register_validated`).
//!   - `cpp_component_subscription_with_safety_delivers_crc_valid_count`
//!     C++ talker + C++ listener (`node.create_subscription_with_safety<M>()`).
//!
//! Run with: `cargo nextest run -p nros-tests --test cpp_c_safety_integrity_e2e`

use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, build_int32_sink, build_native_workspace_c_safety_listener_entry,
    build_native_workspace_c_safety_talker_entry, build_native_workspace_cpp_safety_listener_entry,
    build_native_workspace_cpp_safety_talker_entry, require_zenohd, zenohd_unique,
};
use rstest::rstest;
use std::{process::Command, time::Duration};

/// Spawn a workspace safety entry binary connecting to `locator`, running for `spin_ms`.
fn spawn_entry(
    entry: std::path::PathBuf,
    label: &'static str,
    locator: &str,
    spin_ms: u32,
) -> ManagedProcess {
    let mut cmd = Command::new(entry);
    cmd.env("RUST_LOG", "info")
        .env("NROS_LOCATOR", locator)
        .env("NROS_SESSION_MODE", "client")
        .env("NROS_ENTRY_SPIN_MS", spin_ms.to_string())
        .env("NROS_ENTRY_SPIN_STEP_MS", "10");
    ManagedProcess::spawn_command(cmd, label).unwrap_or_else(|e| panic!("spawn {label}: {e}"))
}

/// Spawn an nros subscriber on `/safe_ok` (prints `Received: <count>` per message).
fn spawn_safe_ok_listener(locator: &str) -> ManagedProcess {
    let listener = build_int32_sink()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("native listener fixture not built: {e}"));
    let mut cmd = Command::new(listener);
    cmd.env("RUST_LOG", "info")
        .env("NROS_LOCATOR", locator)
        .env("NROS_SESSION_MODE", "client")
        .env("NROS_SUB_TOPIC", "/safe_ok");
    let mut proc = ManagedProcess::spawn_command(cmd, "safe-ok-listener").expect("spawn listener");
    proc.wait_for_output_pattern("Listener", Duration::from_secs(8))
        .expect("safe_ok listener did not become ready");
    proc
}

/// W3 (C) — the C component validated-subscription API delivers a climbing CRC-valid
/// count cross-process: `nros_cpp_subscription_register_validated` callback receives
/// `crc_valid == 1` for each integrity-passing frame and republishes the count on /safe_ok.
#[rstest]
fn c_component_subscription_validated_delivers_crc_valid_count(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    let talker = build_native_workspace_c_safety_talker_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("C safety talker fixture not built: {e}"));
    let listener_entry = build_native_workspace_c_safety_listener_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("C safety listener fixture not built: {e}"));
    let locator = zenohd_unique.locator();

    // Start /safe_ok subscriber first so it's ready before any publishes.
    let mut sub = spawn_safe_ok_listener(&locator);
    // Listener must subscribe to /chatter before the talker starts publishing.
    let mut listener = spawn_entry(listener_entry, "c-safety-listener", &locator, 20000);
    std::thread::sleep(Duration::from_millis(1000));
    let mut tlk = spawn_entry(talker, "c-safety-talker", &locator, 20000);

    // At 1 Hz with CRC, each frame validated → /safe_ok count climbs.
    // Waiting for 3 confirms the full validated-callback path is functional.
    let out = sub
        .wait_for_output_count(
            nros_tests::output::INT32_LISTENER_LOG_PREFIX,
            3,
            Duration::from_secs(25),
        )
        .unwrap_or_else(|_| {
            tlk.kill();
            listener.kill();
            sub.kill();
            panic!(
                "/safe_ok never saw 3 CRC-validated publishes — the C component \
                 nros_cpp_subscription_register_validated path failed \
                 (talker → backend CRC → validated C callback → /safe_ok republish)"
            )
        });

    tlk.kill();
    listener.kill();
    sub.kill();

    let n = nros_tests::count_pattern(&out, nros_tests::output::INT32_LISTENER_LOG_PREFIX);
    assert!(
        n >= 3,
        "expected ≥3 CRC-validated /safe_ok publishes, got {n}\n{out}"
    );
}

/// W3 (C++) — the C++ component typed safety subscription API delivers a climbing
/// CRC-valid count cross-process: `node.create_subscription_with_safety<M>()` receives
/// `(const M&, const nros_cpp_integrity_status_t&)` with `crc_valid == 1` per frame.
#[rstest]
fn cpp_component_subscription_with_safety_delivers_crc_valid_count(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    let talker = build_native_workspace_cpp_safety_talker_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("C++ safety talker fixture not built: {e}"));
    let listener_entry = build_native_workspace_cpp_safety_listener_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("C++ safety listener fixture not built: {e}"));
    let locator = zenohd_unique.locator();

    let mut sub = spawn_safe_ok_listener(&locator);
    let mut listener = spawn_entry(listener_entry, "cpp-safety-listener", &locator, 20000);
    std::thread::sleep(Duration::from_millis(1000));
    let mut tlk = spawn_entry(talker, "cpp-safety-talker", &locator, 20000);

    let out = sub
        .wait_for_output_count(
            nros_tests::output::INT32_LISTENER_LOG_PREFIX,
            3,
            Duration::from_secs(25),
        )
        .unwrap_or_else(|_| {
            tlk.kill();
            listener.kill();
            sub.kill();
            panic!(
                "/safe_ok never saw 3 CRC-validated publishes — the C++ component \
                 create_subscription_with_safety path failed \
                 (talker → backend CRC → typed callback (M, IntegrityStatus) → /safe_ok republish)"
            )
        });

    tlk.kill();
    listener.kill();
    sub.kill();

    let n = nros_tests::count_pattern(&out, nros_tests::output::INT32_LISTENER_LOG_PREFIX);
    assert!(
        n >= 3,
        "expected ≥3 CRC-validated /safe_ok publishes, got {n}\n{out}"
    );
}
