//! phase-263 A5 (Track D) — runtime E2E for node logging in the workspace shape.
//!
//! A board-agnostic Node pkg logs via `nros_info!` in its callback, but only the board
//! knows its sink. phase-264 W3 made `nros-board-posix` register the default platform
//! sink (host → stdout/stderr) once at boot, so a Node pkg's `nros_info!` reaches the
//! native entry's stdout with no per-app init.
//!
//! The ws-rust `talker_pkg` logs `"talker publishing chatter seq=<n>"` each tick via
//! `nros_log::nros_info!(&DEFAULT_LOGGER, …)`. Booting the `native_entry` and observing
//! that line on the entry's own stdout proves the whole chain: board boot-time
//! `nros_log::init` → global sink → `DEFAULT_LOGGER.dispatch` → host stdout. (No
//! subscriber is involved — logging is local to the process, unlike pub/sub delivery
//! which is cross-process per issue 0096.)
//!
//! Run with: `cargo nextest run -p nros-tests --test logging_workspace_e2e`

use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, build_native_workspace_rust_entry, require_zenohd, zenohd_unique,
};
use rstest::rstest;
use std::{process::Command, time::Duration};

/// A5 — a Node pkg's `nros_info!` reaches the native entry's stdout (the board's
/// boot-time default sink is live before the user closure).
#[rstest]
fn workspace_node_logging_reaches_stdout(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    let entry = build_native_workspace_rust_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("workspace entry fixture not built: {e}"));
    let locator = zenohd_unique.locator();

    let mut cmd = Command::new(entry);
    cmd.env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator)
        .env("NROS_SESSION_MODE", "client")
        .env("NROS_ENTRY_SPIN_MS", "8000")
        .env("NROS_ENTRY_SPIN_STEP_MS", "10");
    let mut proc = ManagedProcess::spawn_command(cmd, "logging-entry").expect("spawn entry");

    // The talker logs once per 1 Hz tick; 3 lines confirms the sink is live and the
    // node's `nros_info!` keeps reaching stdout across ticks.
    let out = proc
        .wait_for_output_count("talker publishing chatter", 3, Duration::from_secs(18))
        .unwrap_or_else(|_| {
            proc.kill();
            panic!(
                "the workspace node's `nros_info!` never reached the entry's stdout — the \
                 board's boot-time default log sink (phase-264 W3) is not routing node logs"
            )
        });

    proc.kill();

    let n = nros_tests::count_pattern(&out, "talker publishing chatter");
    assert!(n >= 3, "expected ≥3 node log lines on stdout, got {n}");
}
