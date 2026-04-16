//! Bindgen-generated FFI bindings for NuttX POSIX sockets.
//!
//! Types (`struct timeval`, `struct addrinfo`, socket constants) and
//! functions (`socket`, `connect`, etc.) are auto-generated from
//! NuttX's POSIX headers, avoiding manual type size/offset errors.

#![no_std]
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(dead_code)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
