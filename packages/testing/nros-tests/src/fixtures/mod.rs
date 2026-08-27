//! rstest fixtures for integration tests
//!
//! Provides `#[rstest::fixture]` functions for:
//! - `zenohd`, `zenohd_unique` - Managed zenohd router
//! - `qemu_binary`, `talker_binary`, `listener_binary` - Binary build fixtures
//!
//! Also re-exports utilities from sibling modules for convenience.

mod binaries;
pub mod cache_key;
pub mod groups;
pub mod lane;
pub mod staleness;
pub mod tls_certs;
#[allow(hidden_glob_reexports)] // rstest fixture creates a module matching the fn name
mod xrce_agent;
mod zenohd_router;

pub use binaries::*;
pub use tls_certs::*;
pub use xrce_agent::*;
pub use zenohd_router::*;

// Re-export utilities for backwards compatibility
pub use crate::{esp32::*, process::*, qemu::*, ros2::*, zephyr::*};

/// Whether test fixtures (zenohd router, XRCE Agent, …) should capture their
/// stdout/stderr to a log file. **Off by default** (the fixture uses a null
/// sink), so a normal test run leaves no per-fixture logs behind. Set
/// `NROS_TEST_LOGS=1` to turn capture on. This is the single "logs only when
/// needed" switch.
pub fn fixture_logs_enabled() -> bool {
    std::env::var_os("NROS_TEST_LOGS").is_some()
}

/// Unified log path for a test fixture: `<name>.log` under one directory —
/// `test-logs/fixtures/` by default, or `$NROS_TEST_LOG_DIR` if set (the dir is
/// created). Replaces the old scattered `/tmp/zenohd-*.log` /
/// `<repo>/xrce-agent-*.log` files so captured logs collect in one place instead
/// of exploding across `/tmp`.
pub fn fixture_log_path(name: &str) -> std::path::PathBuf {
    let dir = std::env::var_os("NROS_TEST_LOG_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| crate::project_root().join("test-logs/fixtures"));
    let _ = std::fs::create_dir_all(&dir);
    dir.join(format!("{name}.log"))
}

/// Spawn the shared `int32-sink` listener fixture and WAIT UNTIL IT SUBSCRIBES.
///
/// phase-373 W3. Seven tests carried a private `spawn_listener` doing this; the
/// copies had drifted into two behaviours, and the more common one was racy.
///
/// ## The race the private copies had
///
/// Four of them waited on the literal `"Listener"`. The sink's banner does
/// contain that word deliberately — but the banner is the FIRST thing it prints,
/// before `nros::init`, before `Executor::open`, and ~25 lines before the
/// subscription is built. So those tests resumed as soon as the process emitted
/// any log line at all, then published into a session that might not have a
/// subscriber yet. `param_live_read_e2e` even carried the comment
/// "Subscription must be live before the talker publishes" directly above a wait
/// that did not ensure it.
///
/// [`crate::output::INT32_SINK_READY_MARKER`] (`"Waiting for Int32"`) is printed
/// AFTER `subscription(...).build(...)` returns, which is what "ready" has to
/// mean here. This is exactly why the repo rule says to grep
/// `nros_tests::output::*` constants and never literal strings: a literal can go
/// on matching the wrong line forever, and nothing points at the mismatch.
///
/// `topic` is `None` for the sink's default `/chatter`, or `Some(t)` to observe
/// another Int32 topic via `NROS_SUB_TOPIC`.
pub fn spawn_int32_sink(topic: Option<&str>, locator: &str) -> crate::process::ManagedProcess {
    use std::{process::Command, time::Duration};

    let listener = build_int32_sink()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| crate::skip!("int32-sink fixture not built: {e}"));

    let label = topic.unwrap_or("listener").to_string();
    let mut cmd = Command::new(listener);
    cmd.env("RUST_LOG", "info")
        .env("NROS_LOCATOR", locator)
        .env("NROS_SESSION_MODE", "client");
    if let Some(t) = topic {
        cmd.env("NROS_SUB_TOPIC", t);
    }

    let mut proc = crate::process::ManagedProcess::spawn_command(cmd, &label)
        .unwrap_or_else(|e| panic!("spawn {label}: {e}"));
    proc.wait_for_output_pattern(
        crate::output::INT32_SINK_READY_MARKER,
        Duration::from_secs(10),
    )
    .unwrap_or_else(|_| {
        panic!("{label} did not subscribe within 10 s (no `INT32_SINK_READY_MARKER`)")
    });
    proc
}

/// A directory under the shared fixture tree,
/// `packages/testing/nros-tests/fixtures/<name>`.
///
/// phase-373 W3. Fifteen tests spelled that path themselves, usually as
/// `project_root().join("packages/testing/nros-tests/fixtures/<name>")` wrapped
/// in a private `fixture_dir()`. The fixture NAME differs per test and belongs
/// to the test; where the fixture tree LIVES is one fact this crate owns, and
/// fifteen copies of it is fifteen edits the day it moves.
///
/// Note there is a second, one-entry tree at `nros-tests/tests/fixtures/`
/// reached via `CARGO_MANIFEST_DIR` (`cross_libc_precedence`). This helper does
/// NOT cover it: consolidating the two means moving a fixture, which is a
/// content change rather than a call-site one, and is left as its own item.
pub fn fixture_dir(name: &str) -> std::path::PathBuf {
    crate::project_root()
        .join("packages/testing/nros-tests/fixtures")
        .join(name)
}
