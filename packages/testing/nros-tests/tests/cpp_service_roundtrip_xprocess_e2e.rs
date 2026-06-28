//! phase-263 A1 (services, C++ projection) — cross-process AddTwoInts service round-trip in the
//! pure-C++ workspace. The C++ sibling of the Rust `service_roundtrip_xprocess_e2e`.
//!
//! Issue 0096: an in-process (same-executor) service server+client can't talk, so the server
//! (`cpp_add_server_pkg`) and client (`cpp_add_client_pkg`) run as TWO processes — one single-node
//! entry each (`native_service_{server,client}_entry`, booting
//! `service_{server,client}.launch.xml`). The client calls `/add_two_ints` each tick with
//! `a = 0,1,2,…`, `b = 1`; the server computes `a + b` and replies; the client prints the
//! server-computed sum it receives. Asserting on the client's stdout proves the FULL
//! cross-process round-trip: request → (separate server process) compute → reply → client.
//!
//! C uses the POLL-model component service client (`nros_cpp_service_client_send_request` +
//! `try_recv_reply`), not Rust's blocking `call_for_name` — a component callback must never
//! block the executor.
//!
//! Run with: `cargo nextest run -p nros-tests --test cpp_service_roundtrip_xprocess_e2e`

use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, build_native_workspace_cpp_service_client_entry,
    build_native_workspace_cpp_service_server_entry, require_zenohd,
};
use std::{process::Command, time::Duration};

const SERVICE_PORT: u16 = 17872;

#[test]
fn cpp_service_roundtrip_delivers_cross_process() {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let server = build_native_workspace_cpp_service_server_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("C++ service server entry not built: {e}"));
    let client = build_native_workspace_cpp_service_client_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("C++ service client entry not built: {e}"));

    let router = ZenohRouter::start_on("0.0.0.0", SERVICE_PORT)
        .unwrap_or_else(|e| nros_tests::skip!("zenohd failed to start on {SERVICE_PORT}: {e}"));
    let locator = format!("tcp/127.0.0.1:{SERVICE_PORT}");
    let _ = router;

    // Server first — it must be discoverable before the client's requests land.
    let mut srv = {
        let mut cmd = Command::new(&server);
        cmd.env("NROS_LOCATOR", &locator);
        ManagedProcess::spawn_command(cmd, "c-add-server")
            .unwrap_or_else(|e| panic!("spawn server: {e}"))
    };
    srv.wait_for_output_pattern("server ready", Duration::from_secs(10))
        .unwrap_or_else(|_| {
            srv.kill();
            panic!("c_add_server never became ready")
        });

    let mut cli = {
        let mut cmd = Command::new(&client);
        cmd.env("NROS_LOCATOR", &locator);
        ManagedProcess::spawn_command(cmd, "c-add-client")
            .unwrap_or_else(|e| panic!("spawn client: {e}"))
    };

    let out = cli
        .wait_for_output_count("sum:", 3, Duration::from_secs(60))
        .unwrap_or_else(|_| {
            cli.kill();
            srv.kill();
            panic!(
                "c_add_client never received 3 server-computed sums — the cross-process C++ \
                 service round-trip did not work"
            )
        });

    cli.kill();
    srv.kill();

    // The server computes a + b = a + 1 for a = 0,1,2,…, so the first three sums the client
    // prints are 1, 2, 3 (early pre-discovery requests may be dropped + resent, so assert the
    // VALUES appear, not a strict prefix).
    for expected in ["sum: 1", "sum: 2", "sum: 3"] {
        assert!(
            out.contains(expected),
            "client output missing `{expected}` — server-side compute or round-trip wrong.\n{out}"
        );
    }
    let n = nros_tests::count_pattern(&out, "sum:");
    assert!(n >= 3, "expected ≥3 service round-trips, got {n}");
}
