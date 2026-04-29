//! `unix-mock` backend — Unix `SOCK_DGRAM` socketpair simulating one IVC
//! channel.
//!
//! The mock is designed for two consumers:
//!
//! 1. **In-process loopback tests** (this crate's `tests/loopback.rs`,
//!    plus consumers in `nros-platform-orin-spe` and `autoware_sentinel`
//!    Stage 1). Use [`register_pair`] to wire two channel IDs to the
//!    two ends of a single `UnixDatagram::pair()`, then open both with
//!    [`Channel::open`](crate::Channel::open) — one acts as SPE side,
//!    the other as CCPLEX side.
//!
//! 2. **Cross-process bring-up** (autoware_sentinel's
//!    `src/ivc-bridge/`'s `unix-mock` backend, where the bridge daemon
//!    is a separate process from the FreeRTOS POSIX sentinel). Each
//!    process registers its own end of an AF_UNIX *connected* pair
//!    over the network namespace, again under one channel ID. The
//!    cross-process connection helper lives in the bridge crate, not
//!    here — this driver just maps an arbitrary fd to a channel ID
//!    via [`register_fd`].
//!
//! Frame size is fixed at 64 bytes to match the NVIDIA IVC default.
//! Datagram boundaries are preserved by `SOCK_DGRAM`, so each
//! `write` ↔ one `read` — the link layer sees exactly the frame
//! shape it would on hardware.

use core::ffi::c_void;
use std::os::fd::{AsRawFd, IntoRawFd, RawFd};
use std::os::unix::net::UnixDatagram;
use std::sync::Mutex;

const FRAME_SIZE: u32 = 64;
const MAX_CHANNELS: usize = 16;

struct MockChannel {
    id: u32,
    fd: RawFd,
}

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
        let boxed = Box::new(MockChannel { id, fd });
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
// Backend hooks called from `lib.rs`.
// =============================================================================

pub(crate) fn channel_get(id: u32) -> *mut c_void {
    let reg = REGISTRY.lock().expect("nvidia-ivc unix-mock registry poisoned");
    match reg.lookup(id) {
        Some(c) => c as *const MockChannel as *mut c_void,
        None => core::ptr::null_mut(),
    }
}

pub(crate) unsafe fn read(ch: *mut c_void, buf: *mut u8, len: usize) -> usize {
    if ch.is_null() || buf.is_null() || len == 0 {
        return usize::MAX;
    }
    let mc = unsafe { &*(ch as *const MockChannel) };
    let n = unsafe { recv_nonblocking(mc.fd, buf, len) };
    match n {
        Ok(Some(n)) => n,
        Ok(None) => 0,
        Err(()) => usize::MAX,
    }
}

pub(crate) unsafe fn write(ch: *mut c_void, buf: *const u8, len: usize) -> usize {
    if ch.is_null() || buf.is_null() || len == 0 {
        return usize::MAX;
    }
    let mc = unsafe { &*(ch as *const MockChannel) };
    let n = unsafe { send_dgram(mc.fd, buf, len) };
    match n {
        Ok(n) => n,
        Err(()) => usize::MAX,
    }
}

pub(crate) unsafe fn notify(_ch: *mut c_void) {
    // SOCK_DGRAM wakes the peer naturally — no explicit doorbell.
}

pub(crate) unsafe fn frame_size(_ch: *mut c_void) -> u32 {
    FRAME_SIZE
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
