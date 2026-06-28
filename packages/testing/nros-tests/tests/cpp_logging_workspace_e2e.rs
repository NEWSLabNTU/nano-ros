//! phase-263 A5 (C++ projection) — runtime E2E for node logging in the cpp workspace shape.
//!
//! The cpp `talker` logs `"cpp_talker logging seq=<n>"` each tick via the nros-log facade
//! (`NROS_LOG_INFO(nros_log_default_logger(), …)`). Booting the existing `native_entry` and
//! observing that line on the entry's own output proves the chain: the C/C++ log facade
//! (`nros_log_emit_fmt` → the built-in DEFAULT_LOGGER, level Info) → lazy-installed default sink
//! → the posix platform writer → `[INFO] nros: cpp_talker logging seq=N`. The C projection of the Rust A5
//! `logging_workspace_e2e` (process-local; no subscriber — issue 0096).
//!
//! KEY (A5 C/C++ finding): `NROS_LOG_INFO(NULL, …)` DROPS the record — a real logger handle is
//! required; `nros_log_default_logger()` is the built-in one.
//!
//! Run with: `cargo nextest run -p nros-tests --test cpp_logging_workspace_e2e`

use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, build_native_workspace_cpp_entry, require_zenohd,
};
use std::{process::Command, time::Duration};

const LOG_PORT: u16 = 17882;

#[test]
fn cpp_workspace_node_logging_reaches_output() {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    let entry = build_native_workspace_cpp_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("cpp workspace entry not built: {e}"));

    let router = ZenohRouter::start_on("0.0.0.0", LOG_PORT)
        .unwrap_or_else(|e| nros_tests::skip!("zenohd failed to start on {LOG_PORT}: {e}"));
    let locator = format!("tcp/127.0.0.1:{LOG_PORT}");
    let _ = router;

    let mut cmd = Command::new(&entry);
    cmd.env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator)
        .env("NROS_SESSION_MODE", "client")
        .env("NROS_ENTRY_SPIN_MS", "8000");
    let mut proc = ManagedProcess::spawn_command(cmd, "cpp-logging-entry").expect("spawn entry");

    let out = proc
        .wait_for_output_count("cpp_talker logging", 3, Duration::from_secs(18))
        .unwrap_or_else(|_| {
            proc.kill();
            panic!(
                "the cpp workspace node's NROS_LOG_INFO never reached the entry's output —                  the C/C++ node-log facade chain is broken"
            )
        });

    proc.kill();

    let n = nros_tests::count_pattern(&out, "cpp_talker logging");
    assert!(n >= 3, "expected ≥3 node log lines, got {n}");
}
