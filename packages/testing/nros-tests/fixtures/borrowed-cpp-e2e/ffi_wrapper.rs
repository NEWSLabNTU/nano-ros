//! Phase 235.8 — C++ borrowed E2E: compile the generated FFI glue into a
//! staticlib so a C++ driver can exercise the real Rust
//! `nros_cpp_deserialize_*_borrowed` (+ serialize) against the generated header.
//!
//! Prelude mirrors what a generated message crate provides. `nros_cpp_publish_raw`
//! is referenced by the (unused-here) publish fn — provided as a dummy by the
//! C++ driver at final link.
#![no_std]
#![allow(non_camel_case_types, dead_code)]

use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

unsafe extern "C" {
    fn nros_cpp_publish_raw(handle: *mut core::ffi::c_void, data: *const u8, len: usize) -> i32;
}

include!("e2e_msgs_msg_borrowed_ffi.rs");
