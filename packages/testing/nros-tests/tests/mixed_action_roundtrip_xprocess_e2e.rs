//! phase-263 A4 (actions, MIXED projection) — cross-process Fibonacci action round-trip in the
//! MIXED workspace (C server + C client). The MIXED sibling of the Rust
//! `action_roundtrip_xprocess_e2e` / the C `c_action_roundtrip_xprocess_e2e`. The cross-language
//! cpp variant is blocked on the action_msgs cpp-codegen gap (see the phase doc), so the MIXED
//! projection reuses the pure-C Fibonacci pkgs verbatim; cross-language is preserved at the
//! workspace level (the mixed workspace also carries a C talker + C++ listener + Rust heartbeat).
//!
//! Issue 0096: an in-process (same-executor) action server+client can't talk, so the server
//! (`c_fib_server_pkg`) and client (`c_fib_client_pkg`) run as TWO processes — one single-node
//! entry each (`native_action_{server,client}_entry`, booting
//! `action_{server,client}.launch.xml`). The client sends a goal (order = 10) on `/fibonacci`;
//! the server accepts it, computes the Fibonacci sequence `0,1,1,2,3,5,8,13,21,34,55` (order 10,
//! 11 elements), and completes the goal with that result. The client deserializes the result and
//! prints the last element. Asserting on the client's stdout (`result last=55`) proves the FULL
//! cross-process round-trip: send_goal → (separate server process) accept → compute → result →
//! client result callback.
//!
//! C uses the POLL-model component action client (`send_goal_async` / `get_result_async` /
//! `poll` + `set_callbacks`), not Rust's declarative dispatch — a component callback must never
//! block the executor.
//!
//! Run with: `cargo nextest run -p nros-tests --test mixed_action_roundtrip_xprocess_e2e`

use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, build_native_workspace_mixed_action_client_entry,
    build_native_workspace_mixed_action_server_entry, require_zenohd,
};
use std::{process::Command, time::Duration};

const ACTION_PORT: u16 = 17893;

#[test]
fn mixed_action_roundtrip_delivers_cross_process() {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let server = build_native_workspace_mixed_action_server_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("mixed action server entry not built: {e}"));
    let client = build_native_workspace_mixed_action_client_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("mixed action client entry not built: {e}"));

    let router = ZenohRouter::start_on("0.0.0.0", ACTION_PORT)
        .unwrap_or_else(|e| nros_tests::skip!("zenohd failed to start on {ACTION_PORT}: {e}"));
    let locator = format!("tcp/127.0.0.1:{ACTION_PORT}");
    let _ = router;

    // Server first — it must be discoverable before the client's goal lands.
    let mut srv = {
        let mut cmd = Command::new(&server);
        cmd.env("NROS_LOCATOR", &locator);
        ManagedProcess::spawn_command(cmd, "mixed-fib-server")
            .unwrap_or_else(|e| panic!("spawn server: {e}"))
    };
    srv.wait_for_output_pattern("server ready", Duration::from_secs(10))
        .unwrap_or_else(|_| {
            srv.kill();
            panic!("c_fib_server never became ready")
        });

    let mut cli = {
        let mut cmd = Command::new(&client);
        cmd.env("NROS_LOCATOR", &locator);
        ManagedProcess::spawn_command(cmd, "mixed-fib-client")
            .unwrap_or_else(|e| panic!("spawn client: {e}"))
    };

    let out = cli
        .wait_for_output_pattern("result last=55", Duration::from_secs(60))
        .unwrap_or_else(|_| {
            cli.kill();
            srv.kill();
            panic!(
                "c_fib_client never received the server-computed Fibonacci result — the \
                 cross-process mixed action round-trip did not work"
            )
        });

    cli.kill();
    srv.kill();

    // order = 10 → 0,1,1,2,3,5,8,13,21,34,55, so the result's last element is 55.
    assert!(
        out.contains("result last=55"),
        "client output missing `result last=55` — server-side compute or round-trip wrong.\n{out}"
    );
}
