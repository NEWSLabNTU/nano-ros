//! zenoh-pico smoltcp transport — thin wrapper over nros-smoltcp
//!
//! Provides the `_z_*` C symbols that zenoh-pico expects for TCP/UDP
//! networking, delegating to [`nros_smoltcp::SmoltcpBridge`] for the
//! actual socket management and data transfer.
//!
//! Board crates should depend on `nros-smoltcp` directly for setup
//! (init, socket creation, poll callback). This crate only provides
//! the zenoh-pico-specific FFI symbols.

#![no_std]
// When `no-export` is active, the _z_* functions are not #[no_mangle] and
// may appear unused. The symbols are still needed when no-export is off.
#![cfg_attr(feature = "no-export", allow(dead_code))]

mod tcp;
mod udp;

// Re-export everything from nros-smoltcp for backward compatibility.
// Board crates that currently depend on zpico-smoltcp can migrate to
// nros-smoltcp at their own pace.
pub use nros_smoltcp::*;

/// FFI export: poll the network stack via the registered callback.
///
/// Called from `system.c`'s `z_sleep_ms` implementation.
#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_poll() -> i32 {
    nros_smoltcp::do_poll()
}

/// FFI export: initialize the smoltcp bridge.
///
/// Called from zpico.c's `zpico_init_with_config()`.
#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_init() -> i32 {
    if nros_smoltcp::SmoltcpBridge::is_initialized() {
        return 0;
    }
    nros_smoltcp::SmoltcpBridge::init();
    0
}

/// FFI export: cleanup the smoltcp bridge.
///
/// Called from zpico.c's `zpico_close()`.
#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_cleanup() {
    // Static allocations — nothing to clean up
}
