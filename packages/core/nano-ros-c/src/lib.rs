//! # nano-ros-c
//!
//! C API for nano-ros, providing an rclc-compatible interface for embedded systems.
//!
//! This crate exposes the nano-ros functionality through a C-compatible FFI interface,
//! allowing C applications to use nano-ros with familiar ROS 2 patterns.
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

#[cfg(all(
    feature = "alloc",
    not(any(feature = "platform-posix", feature = "platform-zephyr"))
))]
compile_error!(
    "nano-ros-c `alloc` requires a transport backend. Enable `platform-posix` or `platform-zephyr`."
);

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;

mod action;
mod cdr;
mod clock;
mod constants;
mod error;
mod executor;
mod guard_condition;
mod lifecycle;
mod node;
mod parameter;
mod platform;
mod publisher;
mod qos;
mod service;
mod subscription;
mod support;
mod timer;

// Re-export all public C API items
pub use action::*;
pub use cdr::*;
pub use clock::*;
pub use constants::*;
pub use error::*;
pub use executor::*;
pub use guard_condition::*;
pub use lifecycle::*;
pub use node::*;
pub use parameter::*;
pub use publisher::*;
pub use qos::*;
pub use service::*;
pub use subscription::*;
pub use support::*;
pub use timer::*;
