//! Socket helper stubs for zenoh-pico
//!
//! These are auxiliary socket functions that zenoh-pico expects.
//! The main TCP socket operations (_z_open_tcp, etc.) are in the transport crate.

/// TCP socket type (matches zenoh_bare_metal_platform.h)
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

/// z_result_t _z_socket_set_non_blocking(const _z_sys_net_socket_t *sock)
#[unsafe(no_mangle)]
pub extern "C" fn _z_socket_set_non_blocking(_sock: *const ZSysNetSocket) -> i8 {
    // smoltcp sockets are inherently non-blocking
    0
}

/// z_result_t _z_socket_accept(const _z_sys_net_socket_t *sock_in, _z_sys_net_socket_t *sock_out)
#[unsafe(no_mangle)]
pub extern "C" fn _z_socket_accept(
    _sock_in: *const ZSysNetSocket,
    _sock_out: *mut ZSysNetSocket,
) -> i8 {
    // Not implemented for client-mode connections
    -1 // _Z_ERR_GENERIC
}

/// void _z_socket_close(_z_sys_net_socket_t *sock)
#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn _z_socket_close(sock: *mut ZSysNetSocket) {
    if sock.is_null() {
        return;
    }
    let handle = unsafe { (*sock)._handle };
    if handle >= 0 {
        // Forward to the transport crate's TCP close
        unsafe extern "C" {
            fn _z_close_tcp(sock: *mut ZSysNetSocket);
        }
        unsafe {
            _z_close_tcp(sock);
        }
    }
}

/// z_result_t _z_socket_wait_event(void *peers, _z_mutex_rec_t *mutex)
#[unsafe(no_mangle)]
pub extern "C" fn _z_socket_wait_event(_peers: *mut core::ffi::c_void, _mutex: *mut ZMutexRecRef) -> i8 {
    // For single-threaded polling, just poll the network
    nano_ros_link_smoltcp::smoltcp_poll();
    0
}
