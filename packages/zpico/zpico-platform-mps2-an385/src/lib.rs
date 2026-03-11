//! # zpico-platform-mps2-an385
//!
//! Zenoh-pico system primitives for QEMU MPS2-AN385 bare-metal.
//!
//! Provides all zenoh-pico FFI symbols (memory, clock, RNG, sleep,
//! time, threading stubs, socket helpers, C library stubs) and
//! the network poll callback.
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
pub mod timing;
