//! Phase 244 E2 — reusable custom-transport callback factories.
//!
//! The custom-transport vtable ([`nros_rmw::NrosTransportOps`]) is a set of four
//! `extern "C"` callbacks + an opaque `user_data` that must outlive the zenoh
//! session. Examples used to open-code all of that per transport (a TCP bridge,
//! a ring-buffer loopback, …). This crate provides the common ones as one-call
//! factories so an example plugs in a named transport instead:
//!
//! ```ignore
//! let ops = nros_transport_callbacks::tcp_transport_ops("127.0.0.1:7447")?;
//! unsafe { nros_rmw::set_custom_transport(Some(ops))?; }
//! ```
//!
//! Each factory `Box::leak`s its backing state — the custom-transport contract
//! requires `user_data` to live until the transport is torn down (process exit
//! for these single-session examples), so leaking is correct, not a bug.

use core::ffi::c_void;
use std::{
    collections::VecDeque,
    io::{ErrorKind, Read, Write},
    net::{Shutdown, TcpStream},
    sync::Mutex,
    time::Duration,
};

use nros_rmw::{NROS_TRANSPORT_OPS_ABI_VERSION_V1, NrosTransportOps};

// ============================================================================
// TCP bridge — bridge raw zenoh wire bytes to a TCP socket (e.g. a host zenohd)
// ============================================================================

struct TcpBridge {
    stream: TcpStream,
}

unsafe extern "C" fn tcp_open(_ud: *mut c_void, _params: *const c_void) -> i32 {
    // Connected at factory time; nothing to do.
    0
}

unsafe extern "C" fn tcp_close(ud: *mut c_void) {
    let bridge = unsafe { &*(ud as *const TcpBridge) };
    let _ = bridge.stream.shutdown(Shutdown::Both);
}

unsafe extern "C" fn tcp_write(ud: *mut c_void, buf: *const u8, len: usize) -> i32 {
    let bridge = unsafe { &*(ud as *const TcpBridge) };
    let slice = unsafe { std::slice::from_raw_parts(buf, len) };
    // `Write` is implemented for `&TcpStream` → full-duplex with `tcp_read`
    // running concurrently on zenoh-pico's read-task thread (no lock).
    if (&bridge.stream).write_all(slice).is_err() {
        return -1;
    }
    let _ = (&bridge.stream).flush();
    0
}

unsafe extern "C" fn tcp_read(ud: *mut c_void, buf: *mut u8, len: usize, timeout_ms: u32) -> i32 {
    let bridge = unsafe { &*(ud as *const TcpBridge) };
    let slice = unsafe { std::slice::from_raw_parts_mut(buf, len) };
    let to = Duration::from_millis(timeout_ms.max(1) as u64);
    let _ = bridge.stream.set_read_timeout(Some(to));
    match (&bridge.stream).read(slice) {
        Ok(0) => -1, // peer closed
        Ok(n) => n as i32,
        Err(e) if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::TimedOut => 0,
        Err(_) => -1,
    }
}

/// Build a custom-transport vtable that bridges zenoh wire bytes to a TCP
/// endpoint (`target` = `"host:port"`). The socket is connected eagerly; the
/// returned ops are ready to hand to [`nros_rmw::set_custom_transport`].
///
/// zenoh-pico's `Z_LINK_TYPE_CUSTOM` is stream-flow, so the 2-byte length prefix
/// is owned by zenoh-pico — these callbacks just shovel raw bytes.
pub fn tcp_transport_ops(target: &str) -> std::io::Result<NrosTransportOps> {
    let stream = TcpStream::connect(target)?;
    stream.set_nodelay(true)?;
    stream.set_read_timeout(Some(Duration::from_millis(50)))?;
    stream.set_write_timeout(Some(Duration::from_millis(1000)))?;
    let bridge: &'static mut TcpBridge = Box::leak(Box::new(TcpBridge { stream }));
    Ok(NrosTransportOps {
        abi_version: NROS_TRANSPORT_OPS_ABI_VERSION_V1,
        _reserved: 0,
        user_data: bridge as *mut TcpBridge as *mut c_void,
        open: tcp_open,
        close: tcp_close,
        write: tcp_write,
        read: tcp_read,
    })
}

// ============================================================================
// Loopback — an in-process ring buffer; bytes written are read back. Useful for
// transport-path smoke tests with no external peer.
// ============================================================================

struct Loopback {
    buf: Mutex<VecDeque<u8>>,
    capacity: usize,
}

unsafe extern "C" fn lb_open(_ud: *mut c_void, _params: *const c_void) -> i32 {
    0
}

unsafe extern "C" fn lb_close(_ud: *mut c_void) {}

unsafe extern "C" fn lb_write(ud: *mut c_void, buf: *const u8, len: usize) -> i32 {
    let lb = unsafe { &*(ud as *const Loopback) };
    let slice = unsafe { std::slice::from_raw_parts(buf, len) };
    let mut q = match lb.buf.lock() {
        Ok(q) => q,
        Err(_) => return -1,
    };
    if q.len() + slice.len() > lb.capacity {
        return -1; // overflow
    }
    q.extend(slice.iter().copied());
    0
}

unsafe extern "C" fn lb_read(ud: *mut c_void, buf: *mut u8, len: usize, _timeout_ms: u32) -> i32 {
    let lb = unsafe { &*(ud as *const Loopback) };
    let slice = unsafe { std::slice::from_raw_parts_mut(buf, len) };
    let mut q = match lb.buf.lock() {
        Ok(q) => q,
        Err(_) => return -1,
    };
    let mut n = 0;
    while n < slice.len() {
        match q.pop_front() {
            Some(b) => {
                slice[n] = b;
                n += 1;
            }
            None => break,
        }
    }
    n as i32 // 0 when empty (would-block), per the read contract
}

/// Build an in-process loopback custom-transport vtable backed by a bounded ring
/// buffer of `capacity` bytes. Bytes written are read back in order; `read`
/// returns 0 (would-block) when the buffer is empty. No external peer required.
pub fn loopback_transport_ops(capacity: usize) -> NrosTransportOps {
    let lb: &'static mut Loopback = Box::leak(Box::new(Loopback {
        buf: Mutex::new(VecDeque::with_capacity(capacity)),
        capacity,
    }));
    NrosTransportOps {
        abi_version: NROS_TRANSPORT_OPS_ABI_VERSION_V1,
        _reserved: 0,
        user_data: lb as *mut Loopback as *mut c_void,
        open: lb_open,
        close: lb_close,
        write: lb_write,
        read: lb_read,
    }
}
