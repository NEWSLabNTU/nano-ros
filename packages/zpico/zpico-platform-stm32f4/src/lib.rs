//! # zpico-platform-stm32f4
//!
//! Zenoh-pico system primitives for STM32F4 family microcontrollers.
//!
//! Provides all zenoh-pico FFI symbols (memory, clock, RNG, sleep,
//! time, threading stubs, socket helpers, C library stubs).
//! When the `ethernet` feature is enabled, also provides the network
//! poll callback and hardware modules (PHY, pins, timing).
//!
//! This crate has **no nros dependency** — it only provides the
//! platform symbols needed by zenoh-pico via zpico-sys.

#![no_std]

// System primitive modules (provide zenoh-pico FFI symbols)
pub mod clock;
mod libc_stubs;
pub mod memory;
#[cfg(feature = "ethernet")]
pub mod network;
pub mod random;
mod sleep;
#[cfg(feature = "ethernet")]
mod socket;
#[cfg(not(feature = "ethernet"))]
mod socket_stubs;
mod threading;
mod time;

// Hardware modules
pub mod phy;
pub mod pins;
pub mod timing;
