//! # nros-c
//!
//! C API for nros, providing an rclc-compatible interface for embedded systems.
//!
//! This crate exposes the nros functionality through a C-compatible FFI interface,
//! allowing C applications to use nros with familiar ROS 2 patterns.
//!
//! # Safety
//!
//! All unsafe functions in this crate follow C FFI conventions. Callers must:
//! - Ensure all pointers are valid and properly aligned
//! - Follow the initialization/finalization order documented for each type
//! - Not use objects after they have been finalized

#![no_std]
#![allow(non_camel_case_types)]
// FFI crate - many functions are unsafe extern "C" by necessity
#![allow(clippy::missing_safety_doc)]
// Dead code warnings for internal helpers that may be used later
#![allow(dead_code)]
// Edition 2024: This crate is a pure C FFI wrapper with 420+ unsafe operations in
// unsafe extern "C" functions. Adding explicit unsafe blocks would add significant
// verbosity without meaningful safety improvement, since all callers already need
// to provide the necessary safety guarantees.
#![allow(unsafe_op_in_unsafe_fn)]
// Executor spin loops depend on external state changes (e.g., from another thread calling stop)
#![allow(clippy::while_immutable_condition)]

// ── Feature validation (mutual exclusivity) ─────────────────────────────
#[cfg(all(feature = "platform-posix", feature = "platform-zephyr"))]
compile_error!("`platform-posix` and `platform-zephyr` are mutually exclusive.");

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;

// All C API modules require the rmw-zenoh backend.
// Without it the crate compiles as an empty library.
#[cfg(feature = "rmw-zenoh")]
mod action;
#[cfg(feature = "rmw-zenoh")]
mod cdr;
#[cfg(feature = "rmw-zenoh")]
mod clock;
#[cfg(feature = "rmw-zenoh")]
mod constants;
#[cfg(feature = "rmw-zenoh")]
mod error;
#[cfg(feature = "rmw-zenoh")]
mod executor;
#[cfg(feature = "rmw-zenoh")]
mod guard_condition;
#[cfg(feature = "rmw-zenoh")]
mod lifecycle;
#[cfg(feature = "rmw-zenoh")]
mod node;
#[cfg(feature = "rmw-zenoh")]
mod parameter;
#[cfg(feature = "rmw-zenoh")]
mod platform;
#[cfg(feature = "rmw-zenoh")]
mod publisher;
#[cfg(feature = "rmw-zenoh")]
mod qos;
#[cfg(feature = "rmw-zenoh")]
mod service;
#[cfg(feature = "rmw-zenoh")]
mod subscription;
#[cfg(feature = "rmw-zenoh")]
mod support;
#[cfg(feature = "rmw-zenoh")]
mod timer;

// Re-export all public C API items
#[cfg(feature = "rmw-zenoh")]
pub use action::*;
#[cfg(feature = "rmw-zenoh")]
pub use cdr::*;
#[cfg(feature = "rmw-zenoh")]
pub use clock::*;
#[cfg(feature = "rmw-zenoh")]
pub use constants::*;
#[cfg(feature = "rmw-zenoh")]
pub use error::*;
#[cfg(feature = "rmw-zenoh")]
pub use executor::*;
#[cfg(feature = "rmw-zenoh")]
pub use guard_condition::*;
#[cfg(feature = "rmw-zenoh")]
pub use lifecycle::*;
#[cfg(feature = "rmw-zenoh")]
pub use node::*;
#[cfg(feature = "rmw-zenoh")]
pub use parameter::*;
#[cfg(feature = "rmw-zenoh")]
pub use publisher::*;
#[cfg(feature = "rmw-zenoh")]
pub use qos::*;
#[cfg(feature = "rmw-zenoh")]
pub use service::*;
#[cfg(feature = "rmw-zenoh")]
pub use subscription::*;
#[cfg(feature = "rmw-zenoh")]
pub use support::*;
#[cfg(feature = "rmw-zenoh")]
pub use timer::*;
