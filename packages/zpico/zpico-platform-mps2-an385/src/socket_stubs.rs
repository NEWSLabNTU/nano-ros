//! No-op socket stubs for non-ethernet builds
//!
//! When only serial transport is enabled, zenoh-pico's compiled C code
//! still references socket/smoltcp symbols. These stubs satisfy the
//! linker without pulling in the full TCP/IP stack.

/// TCP socket type (matches bare-metal/platform.h)
#[repr(C)]
pub(crate) struct ZSysNetSocket {
    pub _handle: i8,
    pub _connected: bool,
}

/// Opaque mutex type reference for socket wait
#[repr(C)]
pub(crate) struct ZMutexRecRef {
    _unused: u8,
}

#[unsafe(no_mangle)]
pub extern "C" fn _z_socket_set_non_blocking(_sock: *const ZSysNetSocket) -> i8 {
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn _z_socket_accept(
    _sock_in: *const ZSysNetSocket,
    _sock_out: *mut ZSysNetSocket,
) -> i8 {
    -1
}

#[unsafe(no_mangle)]
pub extern "C" fn _z_socket_close(_sock: *mut ZSysNetSocket) {}

#[unsafe(no_mangle)]
pub extern "C" fn _z_socket_wait_event(
    _peers: *mut core::ffi::c_void,
    _mutex: *mut ZMutexRecRef,
) -> i8 {
    0
}

/// No-op smoltcp bridge init (called by zenoh-pico transport init)
#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_init() {}

/// No-op smoltcp bridge cleanup
#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_cleanup() {}

/// Clock stub (serial transport uses z_clock_* symbols directly)
#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_clock_now_ms() -> u64 {
    crate::clock::clock_ms()
}
