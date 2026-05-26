//! ThreadX Linux integration tests
//!
//! Tests that verify ThreadX Linux examples build and run natively.
//! ThreadX Linux examples use the ThreadX Linux simulation port with
//! the nsos-netx driver forwarding `nx_bsd_*` calls to host POSIX sockets.
//!
//! The E2E test bodies live in `tests/rtos_e2e.rs` (parametrised over
//! platform × language × variant); the per-role fixtures are built by
//! `build-all` / `build-test-fixtures` (Phase 181) before `test-all`.
//!
//! Prerequisites:
//! - `THREADX_DIR` env var pointing to ThreadX source (e.g., `third-party/threadx/kernel`)
//! - nsos-netx at `packages/drivers/nsos-netx/`
//!
//! Run with: `just test-threadx-linux`
//! Or: `cargo nextest run -p nros-tests --test threadx_linux`

use nros_tests::fixtures::{
    is_zenohd_available,
    threadx_linux::{is_nsos_netx_available, is_threadx_available},
};

// =============================================================================
// Prerequisite detection tests (always run)
// =============================================================================
//
// (Phase 182.3) `test_threadx_all_examples_build` removed — it rebuilt every
// ThreadX-Linux example, which `build-all` / `build-test-fixtures` already do
// before `test-all` (the `_require-fixtures` preflight). The per-role binaries
// are consumed by the `rtos_e2e` Platform__ThreadxLinux tests.

#[test]
fn test_threadx_detection() {
    let threadx = is_threadx_available();
    let nsos_netx = is_nsos_netx_available();
    let zenohd = is_zenohd_available();
    eprintln!("ThreadX available: {}", threadx);
    eprintln!("nsos-netx available: {}", nsos_netx);
    eprintln!("zenohd available: {}", zenohd);
}
