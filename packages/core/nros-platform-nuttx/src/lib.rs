//! nros platform implementation for NuttX RTOS.
//!
//! NuttX is POSIX-compatible, so this crate delegates entirely to
//! `nros-platform-posix`. All platform methods (clock, alloc, sleep,
//! random, threading) use standard POSIX APIs via the `libc` crate.

/// NuttX platform type — delegates to PosixPlatform.
pub type NuttxPlatform = nros_platform_posix::PosixPlatform;
