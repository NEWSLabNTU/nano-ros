//! `unix-mock` backend — Unix `SOCK_DGRAM` socketpair simulating one IVC
//! channel, with **zero-copy semantics** layered over the kernel
//! syscalls.
//!
//! NVIDIA's FSP IVC API is fundamentally a "borrow ring slot, fill,
//! commit" pattern (see Phase 11.3.A). The mock keeps that pattern at
//! the API surface so consumers don't branch on backend:
//!
//! - Per-channel state owns one TX slot + one RX slot.
//! - `tx_get` returns a pointer into the TX slot; `tx_commit(len)`
//!   calls `send(fd, &tx_slot[..len], 0)` and frees the slot;
//!   `tx_abandon` just clears the in-progress flag.
//! - `rx_get` calls `recv(fd, &mut rx_slot, 0)` non-blocking; on
//!   success returns a pointer into the slot. `rx_release` clears
//!   the in-progress flag (the byte was consumed from the kernel
//!   socket buffer the moment `recv` returned).
//!
//! Datagram boundaries are preserved by `SOCK_DGRAM`, so each
//! `commit` ↔ one `recv` — exactly the frame shape the link layer
//! sees on real hardware. Frame size is fixed at 64 bytes to match
//! the NVIDIA IVC default.
//!
//! Two consumer patterns:
//!
//! 1. **In-process loopback tests** ([`register_pair`]) — wires two
//!    channel IDs to the two ends of a single `UnixDatagram::pair()`.
//! 2. **Cross-process bring-up** ([`register_fd`]) — each side
//!    registers its own end of an AF_UNIX *connected* pair.

use core::cell::Cell;
use core::ffi::c_void;
use std::os::fd::{AsRawFd, IntoRawFd, RawFd};
use std::os::unix::net::UnixDatagram;
use std::sync::Mutex;

const FRAME_SIZE: u32 = 64;
const FRAME_SIZE_USIZE: usize = 64;
const MAX_CHANNELS: usize = 16;

struct MockChannel {
    id: u32,
    fd: RawFd,
    /// Last-recv'd RX frame buffer + length. Populated by `rx_get`,
    /// drained by `rx_release`. The single-slot model mirrors the way
    /// callers (zenoh-pico's link layer) consume one frame at a time.
    rx_slot: Cell<[u8; FRAME_SIZE_USIZE]>,
    rx_len: Cell<usize>,
    rx_in_flight: Cell<bool>,
    /// Pending TX frame buffer. Populated by `tx_get` (handed out as a
    /// pointer), the caller writes into it, then `tx_commit(len)`
    /// flushes via `send(fd, ...)`. `tx_abandon` clears the flag.
    tx_slot: Cell<[u8; FRAME_SIZE_USIZE]>,
    tx_in_flight: Cell<bool>,
}

// SAFETY: Cell isn't Sync, but we serialise all access through
// REGISTRY (Mutex). Cell here is for interior mutability *within* a
// single thread's access to a borrowed MockChannel; aliasing across
// threads is prevented by the registry lock. The mock is dev-only;
// production paths use the FSP backend.
unsafe impl Sync for MockChannel {}

struct Registry {
    /// `Box` keeps each `MockChannel`'s address stable when the Vec
    /// reallocates — `channel_get` hands raw pointers into this storage
    /// across the C ABI, so the addresses must outlive any Vec growth.
    #[allow(clippy::vec_box)]
    channels: Vec<Box<MockChannel>>,
}

impl Registry {
    const fn new() -> Self {
        Self { channels: Vec::new() }
    }

    fn lookup(&self, id: u32) -> Option<&MockChannel> {
        self.channels.iter().find(|c| c.id == id).map(|b| b.as_ref())
    }

    fn insert(&mut self, id: u32, fd: RawFd) -> *mut c_void {
        if self.channels.iter().any(|c| c.id == id) {
            panic!("nvidia-ivc unix-mock: channel id {id} already registered");
        }
        if self.channels.len() >= MAX_CHANNELS {
            panic!("nvidia-ivc unix-mock: at most {MAX_CHANNELS} mock channels");
        }
        let boxed = Box::new(MockChannel {
            id,
            fd,
            rx_slot: Cell::new([0u8; FRAME_SIZE_USIZE]),
            rx_len: Cell::new(0),
            rx_in_flight: Cell::new(false),
            tx_slot: Cell::new([0u8; FRAME_SIZE_USIZE]),
            tx_in_flight: Cell::new(false),
        });
        let ptr = boxed.as_ref() as *const MockChannel as *mut c_void;
        self.channels.push(boxed);
        ptr
    }
}

static REGISTRY: Mutex<Registry> = Mutex::new(Registry::new());

/// Register a single fd as a mock IVC channel under `id`. The caller
/// retains ownership of the *original* socket; the registry duplicates
/// the fd internally. Panics if `id` is already registered.
///
/// Used by cross-process bring-up where each side opens its own end of
/// an AF_UNIX pair.
pub fn register_fd(id: u32, sock: UnixDatagram) {
    sock.set_nonblocking(true)
        .expect("nvidia-ivc unix-mock: set_nonblocking failed");
    let fd = sock.into_raw_fd();
    let mut reg = REGISTRY.lock().expect("nvidia-ivc unix-mock registry poisoned");
    reg.insert(id, fd);
}

/// Wire two channel IDs to the two ends of one `UnixDatagram::pair()`.
/// Both IDs must be unused. Returns nothing; subsequent `Channel::open`
/// calls on either ID will succeed.
///
/// Panics if either ID is already registered.
pub fn register_pair(id_a: u32, id_b: u32) {
    assert!(id_a != id_b, "register_pair: IDs must differ");
    let (a, b) =
        UnixDatagram::pair().expect("nvidia-ivc unix-mock: UnixDatagram::pair failed");
    a.set_nonblocking(true).expect("set_nonblocking a");
    b.set_nonblocking(true).expect("set_nonblocking b");
    let fd_a = a.into_raw_fd();
    let fd_b = b.into_raw_fd();
    let mut reg = REGISTRY.lock().expect("nvidia-ivc unix-mock registry poisoned");
    reg.insert(id_a, fd_a);
    reg.insert(id_b, fd_b);
}

/// Reset the registry — for tests that want a clean slate. Closes every
/// registered fd. Not exposed for production use.
#[doc(hidden)]
pub fn reset_for_tests() {
    let mut reg = REGISTRY.lock().expect("nvidia-ivc unix-mock registry poisoned");
    for c in reg.channels.drain(..) {
        // Reclaim the dup'd fd so we don't leak it across tests.
        unsafe { libc_close(c.fd) };
    }
}

unsafe fn libc_close(fd: RawFd) {
    unsafe extern "C" {
        fn close(fd: RawFd) -> i32;
    }
    let _ = unsafe { close(fd) };
}

// =============================================================================
// Backend hooks called from `lib.rs`. Single-frame outstanding model:
// at most one RX frame and one TX frame in-flight per channel.
// =============================================================================

pub(crate) fn channel_get(id: u32) -> *mut c_void {
    let reg = REGISTRY.lock().expect("nvidia-ivc unix-mock registry poisoned");
    match reg.lookup(id) {
        Some(c) => c as *const MockChannel as *mut c_void,
        None => core::ptr::null_mut(),
    }
}

pub(crate) unsafe fn frame_size(_ch: *mut c_void) -> u32 {
    FRAME_SIZE
}

pub(crate) unsafe fn rx_get(ch: *mut c_void, len_out: *mut usize) -> *const u8 {
    let mc = unsafe { &*(ch as *const MockChannel) };
    if mc.rx_in_flight.get() {
        // Caller violated the protocol — they got a frame and didn't
        // release it before asking for another. Return the same slot
        // would be confusing; refuse instead.
        unsafe { *len_out = 0 };
        return core::ptr::null();
    }
    let mut buf = mc.rx_slot.get();
    match unsafe { recv_nonblocking(mc.fd, buf.as_mut_ptr(), buf.len()) } {
        Ok(Some(n)) => {
            mc.rx_slot.set(buf);
            mc.rx_len.set(n);
            mc.rx_in_flight.set(true);
            unsafe { *len_out = n };
            // Hand the caller a stable pointer into the channel's
            // own slot. Cell isn't Sync but the registry lock
            // serialises access; the slot pointer is valid until
            // `rx_release`.
            mc.rx_slot.as_ptr() as *const u8
        }
        Ok(None) => {
            unsafe { *len_out = 0 };
            core::ptr::null()
        }
        Err(()) => {
            unsafe { *len_out = 0 };
            core::ptr::null()
        }
    }
}

pub(crate) unsafe fn rx_release(ch: *mut c_void) {
    let mc = unsafe { &*(ch as *const MockChannel) };
    mc.rx_in_flight.set(false);
    mc.rx_len.set(0);
}

pub(crate) unsafe fn tx_get(ch: *mut c_void, cap_out: *mut usize) -> *mut u8 {
    let mc = unsafe { &*(ch as *const MockChannel) };
    if mc.tx_in_flight.get() {
        unsafe { *cap_out = 0 };
        return core::ptr::null_mut();
    }
    mc.tx_in_flight.set(true);
    unsafe { *cap_out = FRAME_SIZE_USIZE };
    mc.tx_slot.as_ptr() as *mut u8
}

pub(crate) unsafe fn tx_commit(ch: *mut c_void, len: usize) {
    let mc = unsafe { &*(ch as *const MockChannel) };
    if !mc.tx_in_flight.get() {
        return;
    }
    let buf = mc.tx_slot.get();
    let send_len = len.min(FRAME_SIZE_USIZE);
    let _ = unsafe { send_dgram(mc.fd, buf.as_ptr(), send_len) };
    mc.tx_in_flight.set(false);
}

pub(crate) unsafe fn tx_abandon(ch: *mut c_void) {
    let mc = unsafe { &*(ch as *const MockChannel) };
    mc.tx_in_flight.set(false);
}

pub(crate) unsafe fn notify(_ch: *mut c_void) {
    // SOCK_DGRAM wakes the peer naturally — no explicit doorbell.
}

// =============================================================================
// libc shims — kept narrow so we don't pull in an extra crate dep.
// =============================================================================

unsafe fn recv_nonblocking(fd: RawFd, buf: *mut u8, len: usize) -> Result<Option<usize>, ()> {
    unsafe extern "C" {
        fn recv(fd: RawFd, buf: *mut u8, len: usize, flags: i32) -> isize;
        fn __errno_location() -> *mut i32;
    }
    const MSG_DONTWAIT: i32 = 0x40;
    const EAGAIN: i32 = 11;
    const EWOULDBLOCK: i32 = EAGAIN;
    let n = unsafe { recv(fd, buf, len, MSG_DONTWAIT) };
    if n >= 0 {
        Ok(Some(n as usize))
    } else {
        let err = unsafe { *__errno_location() };
        if err == EAGAIN || err == EWOULDBLOCK {
            Ok(None)
        } else {
            Err(())
        }
    }
}

unsafe fn send_dgram(fd: RawFd, buf: *const u8, len: usize) -> Result<usize, ()> {
    unsafe extern "C" {
        fn send(fd: RawFd, buf: *const u8, len: usize, flags: i32) -> isize;
    }
    let n = unsafe { send(fd, buf, len, 0) };
    if n >= 0 { Ok(n as usize) } else { Err(()) }
}

// AsRawFd is only used to keep the type bound documentary; trim the
// unused-imports warning when the only consumer is the cross-process
// bring-up path that we don't exercise yet.
#[allow(dead_code)]
fn _assert_unix_datagram_is_as_raw_fd<T: AsRawFd>(_: &T) {}
