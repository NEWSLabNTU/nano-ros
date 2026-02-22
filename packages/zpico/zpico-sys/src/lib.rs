//! zpico-sys: C shim library for zenoh-pico with FFI bindings
//!
//! This crate provides:
//! - The compiled C shim library (zenoh_shim.c)
//! - FFI constants and types
//! - zenoh-pico library (compiled from submodule)
//!
//! # Platform Backends
//!
//! Select one backend via feature flags:
//! - `posix` - Uses POSIX threads, for desktop testing
//! - `zephyr` - Uses Zephyr RTOS threads
//! - `bare-metal` - Uses polling (bare-metal platforms)
//! - `freertos` - Uses FreeRTOS threads + lwIP sockets

#![no_std]

#[cfg(any(feature = "std", test))]
extern crate std;

// Note: The smoltcp platform uses a custom bump allocator for C FFI (zenoh-pico),
// not Rust's global allocator. The `alloc` crate is NOT needed.

#[cfg(any(
    feature = "posix",
    feature = "zephyr",
    feature = "bare-metal",
    feature = "freertos"
))]
use core::ffi::c_void;

// ============================================================================
// Configuration Constants
// ============================================================================

pub mod config;
pub use config::*;

// ============================================================================
// FFI Declarations
// ============================================================================

mod ffi;
pub use ffi::*;

// ============================================================================
// Platform-specific Modules
// ============================================================================

// Note: The C platform layer (`c/platform/`) provides bare-metal
// headers and optional C shims. Platform crates (`zpico-platform-*`)
// provide system primitives (clock, memory, RNG) and the transport
// crate provides TCP symbols directly in Rust.

// ============================================================================
// Extern C Functions from the Shim
// ============================================================================

/// A key-value property for transport configuration (C-compatible)
#[repr(C)]
pub struct zenoh_shim_property_t {
    /// Property key (null-terminated C string)
    pub key: *const core::ffi::c_char,
    /// Property value (null-terminated C string)
    pub value: *const core::ffi::c_char,
}

// These extern declarations import the C shim functions.
// The actual implementations are in c/shim/zenoh_shim.c
//
// Note: Excluded from cbindgen - these are Rust imports of C functions,
// not declarations for the header file.
#[cfg(all(
    any(
        feature = "posix",
        feature = "zephyr",
        feature = "bare-metal",
        feature = "freertos"
    ),
    not(cbindgen)
))]
#[allow(improper_ctypes)]
unsafe extern "C" {
    // Session lifecycle
    pub fn zenoh_shim_init(locator: *const core::ffi::c_char) -> i32;
    pub fn zenoh_shim_init_with_config(
        locator: *const core::ffi::c_char,
        mode: *const core::ffi::c_char,
        properties: *const zenoh_shim_property_t,
        num_properties: usize,
    ) -> i32;
    pub fn zenoh_shim_open() -> i32;
    pub fn zenoh_shim_is_open() -> i32;
    pub fn zenoh_shim_close();

    // ZenohId
    pub fn zenoh_shim_get_zid(zid_out: *mut u8) -> i32;

    // Publishers
    pub fn zenoh_shim_declare_publisher(keyexpr: *const core::ffi::c_char) -> i32;
    pub fn zenoh_shim_publish(handle: i32, data: *const u8, len: usize) -> i32;
    pub fn zenoh_shim_publish_with_attachment(
        handle: i32,
        data: *const u8,
        len: usize,
        attachment: *const u8,
        attachment_len: usize,
    ) -> i32;
    pub fn zenoh_shim_undeclare_publisher(handle: i32) -> i32;

    // Subscribers
    pub fn zenoh_shim_declare_subscriber(
        keyexpr: *const core::ffi::c_char,
        callback: ShimCallback,
        ctx: *mut c_void,
    ) -> i32;
    pub fn zenoh_shim_declare_subscriber_with_attachment(
        keyexpr: *const core::ffi::c_char,
        callback: ShimCallbackWithAttachment,
        ctx: *mut c_void,
    ) -> i32;
    pub fn zenoh_shim_declare_subscriber_direct_write(
        keyexpr: *const core::ffi::c_char,
        buf_ptr: *mut u8,
        buf_capacity: usize,
        locked_ptr: *const bool,
        callback: ShimNotifyCallback,
        ctx: *mut c_void,
    ) -> i32;
    pub fn zenoh_shim_subscribe_zero_copy(
        keyexpr: *const core::ffi::c_char,
        callback: ShimZeroCopyCallback,
        ctx: *mut c_void,
    ) -> i32;
    pub fn zenoh_shim_undeclare_subscriber(handle: i32) -> i32;

    // Liveliness
    pub fn zenoh_shim_declare_liveliness(keyexpr: *const core::ffi::c_char) -> i32;
    pub fn zenoh_shim_undeclare_liveliness(handle: i32) -> i32;

    // Queryables (for services)
    pub fn zenoh_shim_declare_queryable(
        keyexpr: *const core::ffi::c_char,
        callback: ShimQueryCallback,
        ctx: *mut c_void,
    ) -> i32;
    pub fn zenoh_shim_undeclare_queryable(handle: i32) -> i32;
    pub fn zenoh_shim_query_reply(
        queryable_handle: i32,
        keyexpr: *const core::ffi::c_char,
        data: *const u8,
        len: usize,
        attachment: *const u8,
        attachment_len: usize,
    ) -> i32;

    // Service client (queries)
    pub fn zenoh_shim_get(
        keyexpr: *const core::ffi::c_char,
        payload: *const u8,
        payload_len: usize,
        reply_buf: *mut u8,
        reply_buf_size: usize,
        timeout_ms: u32,
    ) -> i32;

    // Non-blocking service client (async queries)
    pub fn zenoh_shim_get_start(
        keyexpr: *const core::ffi::c_char,
        payload: *const u8,
        payload_len: usize,
        timeout_ms: u32,
    ) -> i32;
    pub fn zenoh_shim_get_check(handle: i32, reply_buf: *mut u8, reply_buf_size: usize) -> i32;

    // Polling
    pub fn zenoh_shim_poll(timeout_ms: u32) -> i32;
    pub fn zenoh_shim_spin_once(timeout_ms: u32) -> i32;
    pub fn zenoh_shim_uses_polling() -> bool;
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants() {
        assert_eq!(ZPICO_OK, 0);
        assert_eq!(ZPICO_ERR_GENERIC, -1);
        assert!(ZPICO_MAX_PUBLISHERS > 0);
        assert!(ZPICO_MAX_SUBSCRIBERS > 0);
    }
}
