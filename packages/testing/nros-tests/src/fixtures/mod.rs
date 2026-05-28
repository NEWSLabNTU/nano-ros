//! rstest fixtures for integration tests
//!
//! Provides `#[rstest::fixture]` functions for:
//! - `zenohd`, `zenohd_unique` - Managed zenohd router
//! - `qemu_binary`, `talker_binary`, `listener_binary` - Binary build fixtures
//!
//! Also re-exports utilities from sibling modules for convenience.

mod binaries;
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
