//! Panic handler for FreeRTOS C/C++ examples (no_std bare-metal).
//!
//! This is a minimal staticlib that provides the `#[panic_handler]` required
//! by Rust on bare-metal targets. It is linked alongside the nros-c and
//! nros-cpp staticlibs (which are built separately via Corrosion).

#![no_std]

use panic_halt as _;
