//! zenoh-pico over RPMsg, between images on different cores of one SoC.
//!
//! zenoh-pico's RPMsg link is a framed link (one framing with IVC, in the
//! fork's `src/link/unicast/framed.c`): it lends a frame, fills it, commits
//! it, and reads frames back one at a time. This crate answers those calls.
//! What it owns is what every board shares:
//!
//! - the endpoint table, one entry per `rpmsg/<name>` locator a session opens;
//! - a receive ring per endpoint, which the board's receive callback or task
//!   fills through [`nros_rpmsg_deliver`] and the link drains;
//! - the transmit staging frame, sent through the board's `send`.
//!
//! The vendor binding (OpenAMP `rpmsg_*`, TI IPC `RPMessage_*`) is the
//! board's, registered with [`nros_rpmsg_register_backend`]. The C contract
//! is `include/nros/rpmsg_link.h`.
//!
//! Linkage: pulled in by `zpico-sys`'s `link-rpmsg` feature, which also
//! compiles the link (`Z_FEATURE_LINK_RPMSG`) and, because an RPMsg endpoint
//! joins exactly two images, zenoh-pico's point-to-point peer mode.

#![no_std]
// The exports are the C calls zenoh-pico and the board make; their safety
// contracts are the header's.
#![allow(clippy::missing_safety_doc)]

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "unix-mock")]
pub mod unix_mock;

use core::{
    cell::UnsafeCell,
    ffi::{c_char, c_int, c_void},
    ptr,
    sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicUsize, Ordering},
};

/// The largest frame: the RPMsg payload with 512-byte buffers.
/// `NROS_RPMSG_FRAME_SIZE` in the header.
pub const FRAME_SIZE: usize = 496;
/// Endpoints open at once. A peer session on one RPMsg link opens one.
pub const MAX_ENDPOINTS: usize = 2;
/// Received frames an endpoint holds before the link drains them. A 2 KiB
/// zenoh batch is five frames.
pub const RING_SLOTS: usize = 8;

/// The board's RPMsg binding: `nros_rpmsg_backend_t` in the header.
#[repr(C)]
pub struct Backend {
    pub open: Option<
        unsafe extern "C" fn(*mut c_void, *const c_char, usize, *mut Endpoint) -> *mut c_void,
    >,
    pub close: Option<unsafe extern "C" fn(*mut c_void, *mut c_void)>,
    pub send: Option<unsafe extern "C" fn(*mut c_void, *mut c_void, *const u8, usize) -> c_int>,
    pub ctx: *mut c_void,
}

// SAFETY: the table is read-only once registered; `ctx` is the board's to
// make safe to share, as the header states.
unsafe impl Sync for Backend {}

static BACKEND: AtomicPtr<Backend> = AtomicPtr::new(ptr::null_mut());

/// One open endpoint: `nros_rpmsg_link_ep_t` in the header.
///
/// The receive ring is single-producer (the board's delivery) and
/// single-consumer (zenoh-pico's read). `head` and `tail` count frames ever
/// written and read; their difference is the fill.
pub struct Endpoint {
    in_use: AtomicBool,
    board_ep: AtomicPtr<c_void>,
    head: AtomicUsize,
    tail: AtomicUsize,
    dropped: AtomicU32,
    lens: [UnsafeCell<u16>; RING_SLOTS],
    slots: [UnsafeCell<[u8; FRAME_SIZE]>; RING_SLOTS],
    tx: UnsafeCell<[u8; FRAME_SIZE]>,
}

// SAFETY: a slot is written only by the producer while it is outside
// `tail..head`, and read only by the consumer while inside it; the
// Release/Acquire pair on `head`/`tail` orders the copy against the index.
// `tx` belongs to the one zenoh-pico task that writes the link.
unsafe impl Sync for Endpoint {}

impl Endpoint {
    const fn new() -> Self {
        Self {
            in_use: AtomicBool::new(false),
            board_ep: AtomicPtr::new(ptr::null_mut()),
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            dropped: AtomicU32::new(0),
            lens: [const { UnsafeCell::new(0) }; RING_SLOTS],
            slots: [const { UnsafeCell::new([0; FRAME_SIZE]) }; RING_SLOTS],
            tx: UnsafeCell::new([0; FRAME_SIZE]),
        }
    }

    /// Frames dropped on delivery since the endpoint opened.
    pub fn dropped(&self) -> u32 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Copy one received message into the ring. `false` when it was dropped.
    pub fn deliver(&self, data: &[u8]) -> bool {
        if data.len() > FRAME_SIZE {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);
        if head.wrapping_sub(tail) >= RING_SLOTS {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        let i = head % RING_SLOTS;
        // SAFETY: slot `i` is outside `tail..head`, so the consumer is not
        // reading it, and this is the only producer.
        unsafe {
            (&mut *self.slots[i].get())[..data.len()].copy_from_slice(data);
            *self.lens[i].get() = data.len() as u16;
        }
        self.head.store(head.wrapping_add(1), Ordering::Release);
        true
    }
}

static ENDPOINTS: [Endpoint; MAX_ENDPOINTS] = [const { Endpoint::new() }; MAX_ENDPOINTS];

fn backend() -> Option<&'static Backend> {
    // SAFETY: only `nros_rpmsg_register_backend` stores it, from a table the
    // caller promised outlives the program.
    unsafe { BACKEND.load(Ordering::Acquire).as_ref() }
}

/// Install the board's binding: `nros_rpmsg_register_backend`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_rpmsg_register_backend(backend: *const Backend) -> c_int {
    if backend.is_null() {
        return -1;
    }
    match BACKEND.compare_exchange(
        ptr::null_mut(),
        backend.cast_mut(),
        Ordering::AcqRel,
        Ordering::Acquire,
    ) {
        Ok(_) => 0,
        Err(_) => -1,
    }
}

/// Hand one received message to the link: `nros_rpmsg_deliver`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_rpmsg_deliver(
    link_ep: *mut Endpoint,
    data: *const u8,
    len: usize,
) -> c_int {
    // SAFETY: `link_ep` came from `open`; the endpoints are static.
    let Some(ep) = (unsafe { link_ep.as_ref() }) else {
        return -1;
    };
    if data.is_null() && len != 0 {
        return -1;
    }
    let data = if len == 0 {
        &[][..]
    } else {
        // SAFETY: the caller passes `len` readable bytes.
        unsafe { core::slice::from_raw_parts(data, len) }
    };
    if ep.deliver(data) { 0 } else { -1 }
}

// ---------------------------------------------------------------------------
// The calls zenoh-pico's RPMsg link makes (`link/transport/rpmsg.h`).
// ---------------------------------------------------------------------------

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _z_open_rpmsg(name: *const c_char, name_len: usize) -> *mut c_void {
    let Some(b) = backend() else {
        return ptr::null_mut();
    };
    let Some(open) = b.open else {
        return ptr::null_mut();
    };
    let Some(ep) = ENDPOINTS.iter().find(|ep| {
        ep.in_use
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }) else {
        return ptr::null_mut();
    };
    ep.head.store(0, Ordering::Relaxed);
    ep.tail.store(0, Ordering::Relaxed);
    ep.dropped.store(0, Ordering::Relaxed);
    let link_ep = ptr::from_ref(ep).cast_mut();
    // SAFETY: the board's `open`, with the name zenoh-pico parsed from the
    // locator and the endpoint it will deliver to.
    let board_ep = unsafe { open(b.ctx, name, name_len, link_ep) };
    if board_ep.is_null() {
        ep.in_use.store(false, Ordering::Release);
        return ptr::null_mut();
    }
    ep.board_ep.store(board_ep, Ordering::Release);
    link_ep.cast()
}

/// The endpoint behind a channel handle zenoh-pico passes back.
///
/// # Safety
/// `ch` is a handle `_z_open_rpmsg` returned and has not closed.
unsafe fn endpoint<'a>(ch: *mut c_void) -> &'a Endpoint {
    unsafe { &*ch.cast::<Endpoint>() }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _z_close_rpmsg(ch: *mut c_void) {
    if ch.is_null() {
        return;
    }
    let ep = unsafe { endpoint(ch) };
    let board_ep = ep.board_ep.swap(ptr::null_mut(), Ordering::AcqRel);
    if let Some(b) = backend()
        && let Some(close) = b.close
        && !board_ep.is_null()
    {
        // SAFETY: the board's handle from its own `open`.
        unsafe { close(b.ctx, board_ep) };
    }
    ep.in_use.store(false, Ordering::Release);
}

/// RPMsg wakes the other core when a message is sent; there is no separate
/// doorbell.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn _z_rpmsg_notify(_ch: *mut c_void) {}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _z_rpmsg_frame_size(_ch: *mut c_void) -> u32 {
    FRAME_SIZE as u32
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _z_rpmsg_rx_get(ch: *mut c_void, len_out: *mut usize) -> *const u8 {
    let ep = unsafe { endpoint(ch) };
    let tail = ep.tail.load(Ordering::Relaxed);
    let head = ep.head.load(Ordering::Acquire);
    if tail == head {
        return ptr::null();
    }
    let i = tail % RING_SLOTS;
    // SAFETY: slot `i` is inside `tail..head`: written and published by the
    // producer, and not reused until `_z_rpmsg_rx_release` moves `tail`.
    unsafe {
        if !len_out.is_null() {
            *len_out = usize::from(*ep.lens[i].get());
        }
        ep.slots[i].get().cast::<u8>().cast_const()
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _z_rpmsg_rx_release(ch: *mut c_void) {
    let ep = unsafe { endpoint(ch) };
    let tail = ep.tail.load(Ordering::Relaxed);
    if tail != ep.head.load(Ordering::Acquire) {
        ep.tail.store(tail.wrapping_add(1), Ordering::Release);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _z_rpmsg_tx_get(ch: *mut c_void, cap_out: *mut usize) -> *mut u8 {
    let ep = unsafe { endpoint(ch) };
    if !cap_out.is_null() {
        unsafe { *cap_out = FRAME_SIZE };
    }
    ep.tx.get().cast()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _z_rpmsg_tx_commit(ch: *mut c_void, len: usize) -> bool {
    let ep = unsafe { endpoint(ch) };
    let board_ep = ep.board_ep.load(Ordering::Acquire);
    let (Some(b), false) = (backend(), board_ep.is_null()) else {
        return false;
    };
    let Some(send) = b.send else {
        return false;
    };
    if len > FRAME_SIZE {
        return false;
    }
    // SAFETY: the staging frame holds `len` bytes the link just wrote.
    unsafe { send(b.ctx, board_ep, ep.tx.get().cast(), len) == 0 }
}

/// The staging frame is reused by the next `tx_get`; nothing to undo.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn _z_rpmsg_tx_abandon(_ch: *mut c_void) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ring_holds_its_slots_then_drops_and_counts() {
        let ep = Endpoint::new();
        for i in 0..RING_SLOTS {
            assert!(ep.deliver(&[i as u8; 3]));
        }
        assert!(!ep.deliver(&[0xFF]), "a full ring must drop");
        assert_eq!(ep.dropped(), 1);
        let ch = ptr::from_ref(&ep).cast_mut().cast();
        for i in 0..RING_SLOTS {
            let mut n = 0;
            let p = unsafe { _z_rpmsg_rx_get(ch, &mut n) };
            assert!(!p.is_null());
            assert_eq!(n, 3);
            assert_eq!(unsafe { *p }, i as u8, "frames come out in order");
            unsafe { _z_rpmsg_rx_release(ch) };
        }
        assert!(unsafe { _z_rpmsg_rx_get(ch, &mut 0) }.is_null());
        assert!(ep.deliver(&[7]), "a drained ring takes frames again");
    }

    #[test]
    fn a_frame_longer_than_the_rpmsg_payload_is_refused() {
        let ep = Endpoint::new();
        assert!(ep.deliver(&[0; FRAME_SIZE]));
        assert!(!ep.deliver(&[0; FRAME_SIZE + 1]));
        assert_eq!(ep.dropped(), 1);
    }
}
