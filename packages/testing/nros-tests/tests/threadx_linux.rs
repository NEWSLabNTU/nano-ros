//! ThreadX Linux integration tests
//!
//! Tests that verify ThreadX Linux examples build and run natively.
//! ThreadX Linux examples use the ThreadX Linux simulation port with
//! the nsos-netx driver forwarding `nx_bsd_*` calls to host POSIX sockets.
//!
//! The E2E test bodies live in `tests/rtos_e2e.rs` (parametrised over
//! platform × language × variant).
//!
//! Prerequisites:
//! - `THREADX_DIR` env var pointing to ThreadX source (e.g., `third-party/threadx/kernel`)
//! - nsos-netx at `packages/drivers/nsos-netx/`
//!
//! Run with: `just test-threadx-linux`
//! Or: `cargo nextest run -p nros-tests --test threadx_linux`

use nros_tests::fixtures::is_zenohd_available;
use nros_tests::fixtures::threadx_linux::{
    build_threadx_action_client, build_threadx_action_server, build_threadx_listener,
    build_threadx_service_client, build_threadx_service_server, build_threadx_talker,
    is_nsos_netx_available, is_threadx_available,
};

// =============================================================================
// Prerequisite checks
// =============================================================================

/// Skip test if ThreadX build prerequisites are not available
fn require_threadx() -> bool {
    if !is_threadx_available() {
        eprintln!("Skipping test: THREADX_DIR not set or invalid");
        eprintln!("Run: just setup-threadx && source .envrc");
        return false;
    }
    if !is_nsos_netx_available() {
        eprintln!("Skipping test: nsos-netx not found at packages/drivers/nsos-netx/");
        return false;
    }
    true
}

// =============================================================================
// Prerequisite detection tests (always run)
// =============================================================================

#[test]
fn test_threadx_detection() {
    let threadx = is_threadx_available();
    let nsos_netx = is_nsos_netx_available();
    let zenohd = is_zenohd_available();
    eprintln!("ThreadX available: {}", threadx);
    eprintln!("nsos-netx available: {}", nsos_netx);
    eprintln!("zenohd available: {}", zenohd);
}

// =============================================================================
// Build tests (require THREADX_DIR + nsos-netx)
// =============================================================================

#[test]
fn test_threadx_all_examples_build() {
    if !require_threadx() {
        nros_tests::skip!("require_threadx check failed");
    }

    let results = [
        ("talker", build_threadx_talker()),
        ("listener", build_threadx_listener()),
        ("service-server", build_threadx_service_server()),
        ("service-client", build_threadx_service_client()),
        ("action-server", build_threadx_action_server()),
        ("action-client", build_threadx_action_client()),
    ];

    let mut all_ok = true;
    for (name, result) in &results {
        match result {
            Ok(path) => eprintln!("  OK: {} -> {}", name, path.display()),
            Err(e) => {
                eprintln!("  FAIL: {} -> {:?}", name, e);
                all_ok = false;
            }
        }
    }

    assert!(all_ok, "Not all ThreadX Linux examples built successfully");
}
