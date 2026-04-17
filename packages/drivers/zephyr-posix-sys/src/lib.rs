//! Bindgen-generated FFI bindings for Zephyr POSIX sockets.
//!
//! Types and constants are auto-generated from Zephyr's headers using
//! include paths extracted from a Zephyr build tree's compile_commands.json.
//! Requires ZEPHYR_BUILD_DIR to point to a completed Zephyr build.

#![no_std]
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(dead_code)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
