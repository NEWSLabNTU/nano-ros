//! # zpico-platform-stm32f4
//!
//! Zenoh-pico system primitives for STM32F4 family microcontrollers
//! with Ethernet.
//!
//! Provides all zenoh-pico FFI symbols (memory, clock, RNG, sleep,
//! time, threading stubs, socket helpers, C library stubs), the
//! network poll callback, and hardware modules (PHY, pins, timing).
//!
//! This crate has **no nros dependency** — it only provides the
//! platform symbols needed by zenoh-pico via zpico-sys.

#![no_std]

// System primitive modules (provide zenoh-pico FFI symbols)
pub mod clock;
mod libc_stubs;
pub mod memory;
pub mod network;
pub mod random;
mod sleep;
mod socket;
mod threading;
mod time;

// Hardware modules
pub mod phy;
pub mod pins;
pub mod timing;
