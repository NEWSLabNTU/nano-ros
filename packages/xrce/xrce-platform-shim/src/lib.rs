//! Thin `extern "C"` forwarders from XRCE-DDS symbols to nros-platform.
//!
//! This crate is **platform-independent** — the same code works for all
//! platforms. It delegates to `nros_platform::ConcretePlatform`, which
//! resolves to the active platform backend at compile time.
//!
//! XRCE-DDS only requires clock symbols (2-3 total), making this the
//! simplest RMW shim.

#![no_std]

// All symbols require a platform backend to be selected.
// The `active` feature is set by xrce-sys when a platform feature is enabled.
#[cfg(feature = "active")]
mod shim;
