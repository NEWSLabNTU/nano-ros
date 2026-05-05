//! zpico-sys: C wrapper library for zenoh-pico with FFI bindings
//!
//! This crate provides:
//! - The compiled zpico C library (zpico.c)
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

// Force-link the platform shim crate so its extern "C" symbols (z_clock_now,
// z_malloc, _z_mutex_lock, etc.) are available to the C objects in this crate.
// On POSIX/RTOS, `extern crate` suffices. On bare-metal, the board crate must
// also directly depend on zpico-platform-shim for the embedded linker to
// include the symbols (see board crate Cargo.toml).
// Force-link: POSIX/NuttX/FreeRTOS/ThreadX use the shim for platform symbols.
// Bare-metal board crates have their own extern crate for the embedded linker.
#[cfg(any(
    feature = "posix",
    feature = "nuttx",
    feature = "freertos",
    feature = "threadx",
    feature = "orin-spe",
    feature = "zephyr"
))]
extern crate zpico_platform_shim;

// Note: The smoltcp platform uses a custom bump allocator for C FFI (zenoh-pico),
// not Rust's global allocator. The `alloc` crate is NOT needed.

#[cfg(any(
    feature = "posix",
    feature = "zephyr",
    feature = "bare-metal",
    feature = "freertos",
    feature = "nuttx",
    feature = "threadx",
    feature = "orin-spe"
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
pub struct zpico_property_t {
    /// Property key (null-terminated C string)
    pub key: *const core::ffi::c_char,
    /// Property value (null-terminated C string)
    pub value: *const core::ffi::c_char,
}

// These extern declarations import the zpico C functions.
// The actual implementations are in c/zpico/zpico.c
//
// Note: Excluded from cbindgen - these are Rust imports of C functions,
// not declarations for the header file.
#[cfg(all(
    any(
        feature = "posix",
        feature = "zephyr",
        feature = "bare-metal",
        feature = "freertos",
        feature = "nuttx",
        feature = "threadx",
        feature = "orin-spe"
    ),
    not(cbindgen)
))]
#[allow(improper_ctypes)]
unsafe extern "C" {
    // Session lifecycle
    pub fn zpico_init(locator: *const core::ffi::c_char) -> i32;
    pub fn zpico_init_with_config(
        locator: *const core::ffi::c_char,
        mode: *const core::ffi::c_char,
        properties: *const zpico_property_t,
        num_properties: usize,
    ) -> i32;
    pub fn zpico_open() -> i32;
    pub fn zpico_is_open() -> i32;
    pub fn zpico_close();

    // Task scheduling configuration (call between zpico_init and zpico_open)
    pub fn zpico_set_task_config(
        read_priority: u32,
        read_stack_bytes: u32,
        lease_priority: u32,
        lease_stack_bytes: u32,
    );

    // ZenohId
    pub fn zpico_get_zid(zid_out: *mut u8) -> i32;

    // Publishers
    pub fn zpico_declare_publisher(keyexpr: *const core::ffi::c_char) -> i32;
    pub fn zpico_publish(handle: i32, data: *const u8, len: usize) -> i32;
    pub fn zpico_publish_with_attachment(
        handle: i32,
        data: *const u8,
        len: usize,
        attachment: *const u8,
        attachment_len: usize,
    ) -> i32;
    /// Phase 99.F: zero-copy publish via z_bytes_from_static_buf.
    /// Caller guarantees `data` outlives the call.
    pub fn zpico_publish_with_attachment_aliased(
        handle: i32,
        data: *const u8,
        len: usize,
        attachment: *const u8,
        attachment_len: usize,
    ) -> i32;
    pub fn zpico_undeclare_publisher(handle: i32) -> i32;

    // Subscribers
    pub fn zpico_declare_subscriber(
        keyexpr: *const core::ffi::c_char,
        callback: ZpicoCallback,
        ctx: *mut c_void,
    ) -> i32;
    pub fn zpico_declare_subscriber_with_attachment(
        keyexpr: *const core::ffi::c_char,
        callback: ZpicoCallbackWithAttachment,
        ctx: *mut c_void,
    ) -> i32;
    pub fn zpico_declare_subscriber_direct_write(
        keyexpr: *const core::ffi::c_char,
        buf_ptr: *mut u8,
        buf_capacity: usize,
        locked_ptr: *const bool,
        callback: ZpicoNotifyCallback,
        ctx: *mut c_void,
    ) -> i32;
    pub fn zpico_subscribe_zero_copy(
        keyexpr: *const core::ffi::c_char,
        callback: ZpicoZeroCopyCallback,
        ctx: *mut c_void,
    ) -> i32;
    pub fn zpico_undeclare_subscriber(handle: i32) -> i32;

    // Liveliness
    pub fn zpico_declare_liveliness(keyexpr: *const core::ffi::c_char) -> i32;
    pub fn zpico_undeclare_liveliness(handle: i32) -> i32;

    // Queryables (for services)
    pub fn zpico_declare_queryable(
        keyexpr: *const core::ffi::c_char,
        callback: ZpicoQueryCallback,
        ctx: *mut c_void,
    ) -> i32;
    pub fn zpico_undeclare_queryable(handle: i32) -> i32;
    pub fn zpico_query_reply(
        queryable_handle: i32,
        keyexpr: *const core::ffi::c_char,
        data: *const u8,
        len: usize,
        attachment: *const u8,
        attachment_len: usize,
    ) -> i32;

    // Service client (queries)
    pub fn zpico_get(
        keyexpr: *const core::ffi::c_char,
        payload: *const u8,
        payload_len: usize,
        reply_buf: *mut u8,
        reply_buf_size: usize,
        timeout_ms: u32,
    ) -> i32;

    // Non-blocking service client (async queries)
    pub fn zpico_get_start(
        keyexpr: *const core::ffi::c_char,
        payload: *const u8,
        payload_len: usize,
        timeout_ms: u32,
    ) -> i32;
    pub fn zpico_get_check(handle: i32, reply_buf: *mut u8, reply_buf_size: usize) -> i32;

    // Non-blocking liveliness query (for wait_for_service / wait_for_action_server).
    pub fn zpico_liveliness_get_start(keyexpr: *const core::ffi::c_char, timeout_ms: u32) -> i32;
    pub fn zpico_liveliness_get_check(handle: i32) -> i32;
    /// Phase 108.C.zenoh.4-followup — count of liveliness-token
    /// replies on this slot. Used by the subscriber-side
    /// `LivelinessChanged` bridge to surface `alive_count > 1`.
    pub fn zpico_liveliness_get_count(handle: i32) -> i32;

    // Reply waker callback (for async service client)
    pub fn zpico_set_reply_waker(func: Option<unsafe extern "C" fn(i32)>);

    // Polling
    pub fn zpico_spin_once(timeout_ms: u32) -> i32;
    pub fn zpico_uses_polling() -> bool;

    // Clock helpers (for FFI reentrancy guard timeout decomposition)
    pub fn zpico_clock_start(clock_buf: *mut u8);
    pub fn zpico_clock_elapsed_ms_since(clock_buf: *mut u8) -> core::ffi::c_ulong;
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
