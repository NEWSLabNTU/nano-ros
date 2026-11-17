// RFC-0132 D3 (the mock backend) and D5 (never on hardware).

//! The host-only mock backend of the platform CAN frame driver.
//!
//! It defines the five `nros_platform_can_*` symbols of `<nros/platform_can.h>`
//! over an in-process bus: a frame sent is recorded, a frame queued with
//! [`push_rx`] is what `recv` returns, and whether the device carries CAN FD is
//! a setting ([`set_fd_capable`]). An application's encoder can then be tested
//! on the host with no interface at all.
//!
//! # Never on hardware
//!
//! An image selects the mock with `[image.<id>.can] driver = "mock"`, which
//! `nros configure` accepts only on a host-process board. This crate is the
//! compile-time half of the same rule: it is `host-only` in its manifest, and a
//! bare-metal (`target_os = "none"`) or NuttX build that reached it by any path
//! stops at the `compile_error!` below.
//!
//! # One bus per process
//!
//! The C ABI's `open(NULL, ...)` names no instance, so the bus is a process
//! global. Tests that share a process serialise on their own lock and call
//! [`reset`] first.

#![cfg_attr(any(target_os = "none", target_os = "nuttx"), no_std)]

#[cfg(any(target_os = "none", target_os = "nuttx"))]
compile_error!(
    "nros-can-mock is host-only: the CAN mock never reaches \
     hardware. A bare-metal or RTOS image uses the CAN backend its board \
     derives (zephyr, socketcan or the board package's own)."
);

#[cfg(not(any(target_os = "none", target_os = "nuttx")))]
mod imp;
#[cfg(not(any(target_os = "none", target_os = "nuttx")))]
pub use imp::*;
