//! FFI bodies for the bridge's generated `px4_msgs` C++ headers (issue 0362).
//!
//! Mirrors `cmake/ffi_lib_rs.in`, which `nros_generate_interfaces(LANGUAGE CPP)`
//! synthesizes for a normal CMake consumer. A PX4 module builds under PX4's own
//! cmake and never runs that function, so the bridge carries the crate.
//!
//! The generated `_types.rs` / `_exports.rs` pair is written by
//! `nros generate-px4-msgs --lang cpp` into `$NROS_PX4_BRIDGE_GEN/px4_msgs/msg/`
//! and pulled in by the build script, so the topic list is stated ONCE (in the
//! `just` recipe) instead of being restated here.
#![no_std]
#![allow(non_camel_case_types)]

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

unsafe extern "C" {
    fn nros_cpp_publish_raw(handle: *mut core::ffi::c_void, data: *const u8, len: usize) -> i32;
}

/// View a fixed-capacity C string buffer (`char[N]`, NUL-terminated) as a `&str`,
/// stopping at the FIRST NUL — the bytes after the terminator are uninitialized,
/// so validating the whole buffer would spuriously fail and serialize "".
/// PX4's `char[N]` fields (e.g. `debug_key_value.key`) are exactly this shape.
#[allow(dead_code)]
fn fixed_str(buf: &[u8]) -> &str {
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    core::str::from_utf8(&buf[..end]).unwrap_or("")
}

include!(concat!(env!("OUT_DIR"), "/px4_msgs_ffi.rs"));
