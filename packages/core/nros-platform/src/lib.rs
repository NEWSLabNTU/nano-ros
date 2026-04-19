//! Unified platform abstraction traits for nros.
//!
//! This crate defines the backend-agnostic interface that platform
//! implementations (POSIX, Zephyr, FreeRTOS, bare-metal, etc.) must satisfy.
//! RMW backends consume these traits via thin shim crates that translate
//! RMW-specific C symbols (e.g., `z_clock_now`, `uxr_millis`) into calls
//! on the active platform implementation.
//!
//! # Trait hierarchy
//!
//! Capabilities are split into independent sub-traits so each RMW backend
//! can declare exactly what it needs:
//!
//! - [`PlatformClock`] — monotonic clock (required by all backends)
//! - [`PlatformAlloc`] — heap allocation (zenoh-pico only)
//! - [`PlatformSleep`] — sleep / delay (zenoh-pico only)
//! - [`PlatformRandom`] — pseudo-random number generation (zenoh-pico only)
//! - [`PlatformTime`] — wall-clock time (zenoh-pico only)
//! - [`PlatformThreading`] — tasks, mutexes, condvars (multi-threaded platforms)
//!
//! # Compile-time resolution
//!
//! Exactly one platform feature must be enabled. The [`ConcretePlatform`]
//! type alias resolves to the active backend, eliminating generic parameters.

#![no_std]

mod board;
mod resolve;
mod traits;

pub use board::BoardConfig;

#[cfg(any(
    feature = "platform-posix",
    feature = "platform-cffi",
    feature = "platform-mps2-an385",
    feature = "platform-stm32f4",
    feature = "platform-esp32",
    feature = "platform-esp32-qemu",
    feature = "platform-nuttx",
    feature = "platform-freertos",
    feature = "platform-threadx",
    feature = "platform-zephyr",
))]
pub use resolve::ConcretePlatform;
pub use traits::*;
