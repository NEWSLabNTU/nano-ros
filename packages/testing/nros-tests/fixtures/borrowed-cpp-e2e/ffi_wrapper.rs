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

// #0423 — mirror `cmake/ffi_lib_rs.in`'s `fixed_str()`: the production FFI-crate
// lib.rs wrapper provides this helper, and the generated per-message serialize
// routes every non-heap string field through it (message_types.rs.jinja). The
// stub prelude here had drifted without it (which bit-rotted the C++ proof).
fn fixed_str(buf: &[u8]) -> &str {
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    core::str::from_utf8(&buf[..end]).unwrap_or("")
}

// phase-306 W1 — codegen splits the glue into a types + exports pair.
include!("e2e_msgs_msg_borrowed_types.rs");
include!("e2e_msgs_msg_borrowed_exports.rs");
