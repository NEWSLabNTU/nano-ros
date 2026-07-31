//! Bindgen-generated FFI bindings for lwIP BSD sockets on FreeRTOS.
//!
//! Types (`struct timeval`, `struct addrinfo`, socket constants) are
//! auto-generated from the actual C headers, avoiding manual size/offset
//! errors (e.g., newlib's 64-bit `time_t` vs lwIP's 32-bit `long`).

#![no_std]
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(dead_code)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
