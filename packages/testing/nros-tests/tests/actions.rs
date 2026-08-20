//! Action integration tests
//!
//! Tests for ROS 2 action communication between nros nodes.
//!
//! **Bucket (phase-329): KEEP — behavior one-offs, not matrix cells.** The plain
//! rust/zenoh DELIVERY case folded into the native-example req/resp matrix
//! consumer (`native_example_reqresp_e2e.rs`).

use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, action_client_binary, action_server_binary, require_zenohd,
    zenohd_unique,
};
use rstest::rstest;
use std::{path::PathBuf, time::Duration};

// =============================================================================
// Action Server/Client Communication Tests
// =============================================================================

#[rstest]
fn test_action_server_starts(zenohd_unique: ZenohRouter, action_server_binary: PathBuf) {
    use std::process::Command;

    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let locator = zenohd_unique.locator();

    let mut cmd = Command::new(&action_server_binary);
    cmd.env("NROS_LOCATOR", &locator);
    cmd.env("RUST_LOG", "info");
    let mut server = ManagedProcess::spawn_command(cmd, "native-rs-action-server")
        .expect("Failed to start action server");

    // Wait for server readiness. Marker found → fall through to test
    // success. Phase 214.A.3 — dropped the `eprintln!("[PASS]") + return`
    // verbosity; the harness reports PASS on clean fn return.
    if server
        .wait_for_output_pattern(
            nros_tests::output::ACTION_SERVER_READY_MARKER,
            Duration::from_secs(5),
        )
        .is_ok()
    {
        return;
    }

    // Marker not printed within 5s. Distinguish: process still alive
    // = readiness unverified → SKIP (CLAUDE.md-banned to claim PASS on
    // an unmet precondition). Process exited → real failure → panic.
    if server.is_running() {
        nros_tests::skip!(
            "native-rs-action-server did not print 'Waiting for action' marker within 5s"
        );
    } else {
        eprintln!("[FAIL] native-rs-action-server exited early");
        panic!("Action server failed to start");
    }
}

#[rstest]
fn test_action_client_starts(zenohd_unique: ZenohRouter, action_client_binary: PathBuf) {
    use std::process::Command;

    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let locator = zenohd_unique.locator();

    let mut cmd = Command::new(&action_client_binary);
    cmd.env("NROS_LOCATOR", &locator);
    cmd.env("RUST_LOG", "info");
    let mut client = ManagedProcess::spawn_command(cmd, "native-rs-action-client")
        .expect("Failed to start action client");

    // Issue 0711's class — this used to discard the wait result and then print
    // "[PASS] started" or "[INFO] exited (no server available)", passing either
    // way. Both branches were green, so a client that never reached the action
    // API at all read the same as one waiting for a server.
    //
    // The claim that does NOT depend on a server is that the client got far
    // enough to SEND a goal, so assert exactly that and nothing more.
    let startup = client
        .wait_for_output_pattern(
            nros_tests::output::ACTION_SENDING_GOAL_MARKER,
            Duration::from_secs(5),
        )
        .unwrap_or_else(|e| format!("{e}"));
    assert!(
        startup.contains(nros_tests::output::ACTION_SENDING_GOAL_MARKER),
        "action client never reached the goal-send path (still running: {}). \
         No server is expected here — reaching the send IS the claim. \
         Output:\n{startup}",
        client.is_running()
    );
}

// phase-329 W4 — the DELIVERY test (rust/zenoh) FOLDED into the native-example
// req/resp matrix consumer (`native_example_reqresp_e2e.rs`, the
// `(Linux, Rust, Zenoh, Action)` cell). Kept below: server/client startup
// one-offs + the binaries-exist fixture-artifact check.

// =============================================================================

/// The fixture-artifact check: both native action binaries must RESOLVE.
///
/// Both arms used to be `Err(e) => eprintln!("[INFO] Could not build …")` with
/// the only assertion inside `Ok`, so a test named "binaries exist" passed
/// when they did not. Whatever the resolver refuses — absent artifact, STALE
/// against its sources, a manifest row naming a different entry — arrived as an
/// INFO line and a green tick.
///
/// `?` instead: the resolver's error already says which binary, which input was
/// newer, and what to run. That message is the whole value of this test, and
/// printing it while passing threw it away.
#[test]
fn test_action_binaries_exist() -> nros_tests::TestResult<()> {
    use nros_tests::fixtures::{build_native_action_client, build_native_action_server};

    let server = build_native_action_server()?;
    assert!(
        server.exists(),
        "action-server resolved to {} but the file is not there",
        server.display()
    );

    let client = build_native_action_client()?;
    assert!(
        client.exists(),
        "action-client resolved to {} but the file is not there",
        client.display()
    );
    Ok(())
}

/// issue 0461 — the requested `order` must REACH the server.
///
/// Every other action test asserts delivery markers (`Publish feedback`,
/// `Goal succeeded`, a result arriving), none of which depends on the goal
/// payload being decoded correctly. So a server that read a constant for every
/// goal — Rust `1` (the goal_id counter), C/C++ `256` (a CDR header word) —
/// passed all of them, for months, while the value sat 20 bytes further into
/// the buffer. This is the assertion that was missing.
#[rstest]
fn goal_order_reaches_the_server(
    zenohd_unique: ZenohRouter,
    action_server_binary: PathBuf,
    action_client_binary: PathBuf,
) {
    use std::process::Command;

    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    let locator = zenohd_unique.locator();

    let mut scmd = Command::new(&action_server_binary);
    scmd.env("NROS_LOCATOR", &locator);
    let mut server =
        ManagedProcess::spawn_command(scmd, "action-server-order").expect("spawn action server");
    server
        .wait_for_output_pattern(
            nros_tests::output::ACTION_SERVER_READY_MARKER,
            Duration::from_secs(20),
        )
        .expect("action server did not become ready");

    let mut ccmd = Command::new(&action_client_binary);
    ccmd.env("NROS_LOCATOR", &locator);
    let mut client =
        ManagedProcess::spawn_command(ccmd, "action-client-order").expect("spawn action client");

    let server_out = server.collect_until(
        nros_tests::output::ACTION_GOAL_REQUEST_PREFIX,
        Duration::from_secs(30),
    );
    client.kill();
    server.kill();

    let expected = nros_tests::output::ACTION_GOAL_ORDER;
    let seen = nros_tests::output::goal_order_in(&server_out);
    assert_eq!(
        seen,
        Some(expected),
        "the server decoded order {seen:?}, but the client sent {expected}. \
         A constant here means the goal payload is being read from the wrong \
         offset, not that the goal is missing (issue 0461).\nserver:\n{server_out}"
    );
}
