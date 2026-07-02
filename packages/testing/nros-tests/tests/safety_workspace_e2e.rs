//! phase-263 B1 (Track D) — runtime E2E for the E2E-safety (CRC) workspace,
//! cross-process (nros↔nros, no ROS 2 required).
//!
//! In-process node-to-node delivery does not happen (a same-zenoh-session subscriber
//! never receives a same-process publisher; issue 0096), so the safety demo runs the
//! talker and the safe_listener as two processes (matching the imperative cross-process
//! `safety_e2e.rs`):
//!
//!   - `native_safety_talker_entry` boots `talker`; the entry bakes `safety-e2e`, so
//!     each /chatter publish carries a backend-attached CRC-32 + sequence number.
//!   - `native_safety_listener_entry` boots `safe_listener`; its safety subscription
//!     validates the CRC, reads `CallbackCtx::integrity()`, and republishes the running
//!     count of CRC-**valid** messages on `/safe_ok`.
//!
//! An external nros subscriber on `/safe_ok` sees the count climb only while the E2E
//! CRC path holds — proving the differentiator end-to-end: talker publish → backend
//! CRC attach → runtime validate → `integrity().is_valid()` → republish.
//!
//! Run with: `cargo nextest run -p nros-tests --test safety_workspace_e2e`

use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, build_int32_sink,
    build_native_workspace_rust_safety_listener_entry,
    build_native_workspace_rust_safety_talker_entry, require_zenohd, zenohd_unique,
};
use rstest::rstest;
use std::{process::Command, time::Duration};

/// Spawn a workspace safety entry binary on `locator`, spinning for `spin_ms`.
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

/// B1 — the E2E-safety CRC path delivers validated messages end-to-end across
/// processes: the safe_listener republishes a climbing CRC-validated count on `/safe_ok`.
#[rstest]
fn safety_workspace_publishes_crc_validated_count(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    let talker = build_native_workspace_rust_safety_talker_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("safety talker entry fixture not built: {e}"));
    let listener_entry = build_native_workspace_rust_safety_listener_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("safety listener entry fixture not built: {e}"));
    let locator = zenohd_unique.locator();

    let mut sub = spawn_safe_ok_listener(&locator);
    let mut listener = spawn_entry(listener_entry, "safety-listener", &locator, 16000);
    // Let the safe_listener's safety subscription come up before the talker publishes.
    std::thread::sleep(Duration::from_millis(1000));
    let mut tlk = spawn_entry(talker, "safety-talker", &locator, 16000);

    // The talker publishes /chatter at 1 Hz with a CRC; each CRC-validated receive
    // republishes the count on /safe_ok. Seeing 3 confirms the validate path holds.
    let out = sub
        .wait_for_output_count(
            nros_tests::output::INT32_LISTENER_LOG_PREFIX,
            3,
            Duration::from_secs(22),
        )
        .unwrap_or_else(|_| {
            tlk.kill();
            listener.kill();
            sub.kill();
            panic!(
                "/safe_ok never saw 3 CRC-validated publishes — the cross-process E2E safety \
                 path (talker → backend CRC → runtime validate → integrity-read → republish) failed"
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
