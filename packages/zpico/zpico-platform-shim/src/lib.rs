//! Thin `extern "C"` forwarders from zenoh-pico symbols to nros-platform.
//!
//! This crate is **platform-independent** — the same code works for all
//! platforms. It delegates to [`nros_platform::ConcretePlatform`], which
//! resolves to the active platform backend at compile time.
//!
//! Symbols provided:
//! - Clock: `z_clock_now`, `z_clock_elapsed_*`, `z_clock_advance_*`
//! - Memory: `z_malloc`, `z_realloc`, `z_free`
//! - Sleep: `z_sleep_us`, `z_sleep_ms`, `z_sleep_s`
//! - Random: `z_random_u8..u64`, `z_random_fill`
//! - Time: `z_time_now`, `z_time_now_as_str`, `z_time_elapsed_*`, `_z_get_time_since_epoch`
//! - Threading: `_z_task_*`, `_z_mutex_*`, `_z_mutex_rec_*`, `_z_condvar_*`
//! - Socket stubs (smoltcp): `_z_socket_*`

#![no_std]

// All symbols require a platform backend to be selected.
// Without one, this crate compiles as an empty lib.
#[cfg(any(
    feature = "platform-posix",
    feature = "platform-cffi",
    feature = "platform-mps2-an385",
    feature = "platform-stm32f4",
    feature = "platform-esp32",
    feature = "platform-esp32-qemu",
))]
mod shim;
