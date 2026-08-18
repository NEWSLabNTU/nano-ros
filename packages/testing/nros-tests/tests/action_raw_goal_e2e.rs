//! Raw-goal wire proof — issue 0454 / phase-354 W3.
//!
//! `nros_action_client_send_goal_raw` takes a parameter named `goal_cdr` and
//! fed it, unstripped, to a core that appends its own encapsulation header.
//! Two headers reached the wire and every field shifted right by four bytes.
//!
//! The gate `scripts/check-goal-cdr-stripped.py` can only assert that the FFI
//! calls `strip_cdr_header`. This asserts the EFFECT, against a real peer: the
//! C action server prints the order it decoded, and this test reads that number
//! back. Verified by reintroducing the defect: the server then prints 256
//! (`RAW_GOAL_DOUBLE_HEADER_ORDER`) and rejects the goal as out of range. With
//! the fix it prints 7.
//!
//! The assertion deliberately lives on the SERVER's output rather than the
//! probe's. The probe can only report what it believes it sent; only the peer
//! reports what arrived.
//!
//! **Bucket (phase-329): KEEP — a behaviour one-off tied to a specific FFI
//! defect, not a platform/RMW matrix coordinate.** It runs on native zenoh only
//! because that is where the C polling action client has a peer to talk to.

use nros_tests::{
    fixtures::{
        ManagedProcess, ZenohRouter, build_action_raw_goal_probe, c_action_server_binary,
        require_zenohd, zenohd_unique,
    },
    output::{
        ACTION_GOAL_REQUEST_PREFIX, ACTION_SERVER_READY_MARKER, RAW_GOAL_DOUBLE_HEADER_ORDER,
        RAW_GOAL_PROBE_ORDER, RAW_GOAL_SINGLE_HEADER_MARKER, goal_order_in,
    },
};
use rstest::rstest;
use std::{path::PathBuf, process::Command, time::Duration};

/// The probe polls for up to ~20 s of its own budget; give the harness room
/// beyond that so a slow box reports the probe's own diagnosis rather than a
/// harness timeout that hides it.
const PROBE_TIMEOUT: Duration = Duration::from_secs(60);
const SERVER_READY_TIMEOUT: Duration = Duration::from_secs(15);

#[rstest]
fn action_raw_goal_ships_one_cdr_header(
    zenohd_unique: ZenohRouter,
    c_action_server_binary: PathBuf,
) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let probe = match build_action_raw_goal_probe() {
        Ok(p) => p.to_path_buf(),
        Err(e) => panic!("raw-goal probe fixture unavailable: {e}"),
    };

    let locator = zenohd_unique.locator();
    // A NON-ZERO domain, deliberately. Issue 0656 — found by this very test —
    // was the executor-mode server declaring its queryables under `0/…`
    // whatever `ROS_DOMAIN_ID` said, while the polling client the probe uses
    // honoured it: the two sides built different keyexprs and nothing was
    // delivered. 0656 is fixed, so a unique domain is the stronger setup, and
    // it keeps this test standing as the regression check for both facts.
    let domain_id = nros_tests::unique_ros_domain_id().to_string();

    let mut scmd = Command::new(&c_action_server_binary);
    scmd.env("NROS_LOCATOR", &locator);
    scmd.env("ROS_DOMAIN_ID", &domain_id);
    let mut server = ManagedProcess::spawn_command(scmd, "c-action-server-raw-goal")
        .expect("spawn the C action server");

    server
        .wait_for_output_pattern(ACTION_SERVER_READY_MARKER, SERVER_READY_TIMEOUT)
        .expect("C action server never became ready");

    let mut pcmd = Command::new(&probe);
    pcmd.env("NROS_LOCATOR", &locator);
    pcmd.env("ROS_DOMAIN_ID", &domain_id);
    let mut probe_proc =
        ManagedProcess::spawn_command(pcmd, "action-raw-goal-probe").expect("spawn the raw probe");

    let probe_log = probe_proc
        .wait_for_all_output(PROBE_TIMEOUT)
        .unwrap_or_else(|e| panic!("raw-goal probe produced no output within the budget: {e}"));

    // The server's account of what arrived. Collected AFTER the probe finished
    // so the goal line is already in the stream.
    let server_log = server.collect_until(ACTION_GOAL_REQUEST_PREFIX, Duration::from_secs(5));
    server.kill();

    let decoded = goal_order_in(&server_log).unwrap_or_else(|| {
        panic!(
            "the server never logged a decoded goal order.\n\
             --- server ---\n{server_log}\n--- probe ---\n{probe_log}"
        )
    });

    assert_ne!(
        decoded, RAW_GOAL_DOUBLE_HEADER_ORDER,
        "the server decoded {RAW_GOAL_DOUBLE_HEADER_ORDER} — the exact signature of a goal \
         carrying TWO encapsulation headers (issue 0454 regressed).\n\
         --- server ---\n{server_log}\n--- probe ---\n{probe_log}"
    );
    assert_eq!(
        decoded, RAW_GOAL_PROBE_ORDER,
        "the server decoded order {decoded}, not the {RAW_GOAL_PROBE_ORDER} the probe sent — \
         the goal's bytes were reframed in flight.\n\
         --- server ---\n{server_log}\n--- probe ---\n{probe_log}"
    );

    assert!(
        probe_log.contains(RAW_GOAL_SINGLE_HEADER_MARKER),
        "the server decoded the right order but the probe did not complete its own round-trip \
         (accept + a result whose sequence length matches the order).\n\
         --- probe ---\n{probe_log}\n--- server ---\n{server_log}"
    );
}
