//! Bindgen-generated FFI bindings for NetX Duo BSD sockets on ThreadX.
//!
//! Types (`struct nx_bsd_sockaddr_in`, socket constants) and functions
//! (`nx_bsd_socket`, `nx_bsd_connect`, etc.) are auto-generated from
//! the actual C headers, avoiding manual type size/offset errors.

#![no_std]
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(dead_code)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

// htonl/htons/ntohl/ntohs are C macros in NetX — provide as Rust functions.
// On big-endian targets these would be identity; on little-endian they swap.
#[inline]
pub fn htonl(val: u32) -> u32 {
    val.to_be()
}

#[inline]
pub fn htons(val: u16) -> u16 {
    val.to_be()
}

#[inline]
pub fn ntohl(val: u32) -> u32 {
    u32::from_be(val)
}

#[inline]
pub fn ntohs(val: u16) -> u16 {
    u16::from_be(val)
}
