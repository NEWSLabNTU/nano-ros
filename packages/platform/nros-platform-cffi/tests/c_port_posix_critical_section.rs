//! Phase 121.9.e — exercise the POSIX C-port
//! `nros_platform_critical_section_acquire/release` symbols.
//!
//! Validates the canonical critical-section ABI on POSIX:
//!   * Acquire returns a token; release accepts it without UB.
//!   * Nested acquire/release pairs round-trip (token semantics
//!     opaque to caller — POSIX impl is a no-op + 0 token but the
//!     parity test still drives the symbols to lock in their
//!     existence at link time).
//!   * Single-threaded fast-path is hot-path safe: 100k iterations
//!     complete without panic.
//!
//! Run via:
//! ```bash
//! cargo test -p nros-platform-cffi --features posix-c-port \
//!     --test c_port_posix_critical_section
//! ```

#![cfg(feature = "posix-c-port")]

// Force-link the crate's rlib so its `build.rs` cargo:rustc-link-lib
// directives pull in `static=nros_platform_posix`, which carries the
// critical_section symbol bodies. Without this the test binary only
// references raw extern symbols and gnu-ld skips the static archive.
use nros_platform_cffi as _;

unsafe extern "C" {
    fn nros_platform_critical_section_acquire() -> u32;
    fn nros_platform_critical_section_release(token: u32);
}

#[test]
fn acquire_release_round_trip() {
    let token = unsafe { nros_platform_critical_section_acquire() };
    unsafe { nros_platform_critical_section_release(token) };
}

#[test]
fn nested_acquire_release() {
    let outer = unsafe { nros_platform_critical_section_acquire() };
    let inner = unsafe { nros_platform_critical_section_acquire() };
    unsafe { nros_platform_critical_section_release(inner) };
    unsafe { nros_platform_critical_section_release(outer) };
}

#[test]
fn hot_path_stability() {
    for _ in 0..100_000 {
        let token = unsafe { nros_platform_critical_section_acquire() };
        unsafe { nros_platform_critical_section_release(token) };
    }
}
