//! phase-263 A4 (Track D) — runtime E2E for the declarative `Fibonacci` action
//! round-trip, cross-process (nros↔nros, no ROS 2 required).
//!
//! The combined showcase wires `fibonacci_server` + `fibonacci_client` into one entry,
//! but in-process node-to-node delivery does not happen (a same-session subscriber /
//! action server never receives the same-process publisher / client; issue 0096). So a
//! runtime round-trip runs the server and client as two processes (mirroring the
//! imperative cross-process action examples):
//!
//!   - `native_action_server_entry` boots `fibonacci_server` on `/fibonacci`; it accepts
//!     the goal in `on_callback` and drives feedback + result in `tick`.
//!   - `native_action_client_entry` boots `fibonacci_client`, which sends one goal; when
//!     the executor auto-delivers the result, its result callback republishes the **last**
//!     sequence element on `/fib_result`.
//!
//! The server computes the Fibonacci sequence `0,1,1,2,3,5,8,13,21,34,55` (order 10, 11
//! elements), so a correct end-to-end round-trip makes a `/fib_result` subscriber observe
//! `55` — proving the whole chain: client `send_goal` → server accept → compute → result
//! → client result callback → republish.
//!
//! Run with: `cargo nextest run -p nros-tests --test action_roundtrip_xprocess_e2e`

use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, build_int32_sink, build_native_workspace_rust_action_client_entry,
    build_native_workspace_rust_action_server_entry, require_zenohd, zenohd_unique,
};
use rstest::rstest;
use std::{process::Command, time::Duration};

/// Spawn a workspace action entry binary on `locator`, spinning for `spin_ms`.
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

/// Spawn an nros subscriber on `/fib_result` (prints `Received: <n>` per message).
fn spawn_result_listener(locator: &str) -> ManagedProcess {
    let listener = build_int32_sink()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("native listener fixture not built: {e}"));
    let mut cmd = Command::new(listener);
    cmd.env("RUST_LOG", "info")
        .env("NROS_LOCATOR", locator)
        .env("NROS_SESSION_MODE", "client")
        .env("NROS_SUB_TOPIC", "/fib_result");
    let mut proc =
        ManagedProcess::spawn_command(cmd, "fib-result-listener").expect("spawn listener");
    proc.wait_for_output_pattern("Listener", Duration::from_secs(8))
        .expect("fib_result listener did not become ready");
    proc
}

/// A4 — the workspace action round-trip is observable end-to-end across processes:
/// the client's goal is served and the result's last element (55) reaches `/fib_result`.
#[rstest]
fn action_roundtrip_publishes_result_last_element(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    let server = build_native_workspace_rust_action_server_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("action server entry fixture not built: {e}"));
    let client = build_native_workspace_rust_action_client_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("action client entry fixture not built: {e}"));
    let locator = zenohd_unique.locator();

    let mut listener = spawn_result_listener(&locator);
    let mut srv = spawn_entry(server, "action-server", &locator, 20000);
    // Let the server register its action before the client sends a goal.
    std::thread::sleep(Duration::from_millis(1000));
    let mut cli = spawn_entry(client, "action-client", &locator, 20000);

    // The result's last sequence element is 55 (fib up to 11 elements).
    let out = listener
        .wait_for_output_pattern(nros_tests::output::int32_listener_line(55).as_str(), Duration::from_secs(25))
        .unwrap_or_else(|_| {
            cli.kill();
            srv.kill();
            listener.kill();
            panic!(
                "/fib_result never saw the server-computed result (55) — the cross-process \
                 Fibonacci action round-trip (send_goal → accept → result → republish) did not complete"
            )
        });

    cli.kill();
    srv.kill();
    listener.kill();

    assert!(
        out.contains(nros_tests::output::int32_listener_line(55).as_str()),
        "expected the Fibonacci result last element 55 on /fib_result, got:\n{out}"
    );
}
