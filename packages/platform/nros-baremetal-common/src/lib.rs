//! Shared bare-metal helpers for nros-platform-* crates.
//!
//! The 4 bare-metal platform crates (MPS2-AN385, STM32F4, ESP32,
//! ESP32-QEMU) used to each carry byte-identical copies of:
//!
//! - `random.rs` (70 lines × 4 = 280) — xorshift32 PRNG
//! - `sleep.rs`  (44 lines × 4 = 176) — busy-wait with poll callback
//! - `libc_stubs.rs` (247 lines × 2 = 494) — strlen/memcpy/memset/...
//!   for bare-metal targets without a C runtime
//!
//! This crate consolidates them into one place. Platform crates re-
//! export or delegate to these modules.

#![no_std]

pub mod random;
pub mod sleep;

#[cfg(feature = "libc-stubs")]
pub mod libc_stubs;
