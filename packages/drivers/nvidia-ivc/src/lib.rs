//! NVIDIA Tegra IVC driver — `no_std`-friendly safe Rust API plus C-callable
//! `extern "C"` wrappers consumed by zenoh-pico's `Z_FEATURE_LINK_IVC` C
//! code.
//!
//! # Backends
//!
//! Exactly one backend feature must be enabled per build:
//!
//! - `fsp` — links NVIDIA's `tegra_aon_fsp.a` via the `NV_SPE_FSP_DIR`
//!   env var (see `build.rs`). `no_std`, `armv7r-none-eabi`. Requires
//!   SDK Manager creds + a registered `tegra_ivc_channel` instance
//!   from the firmware (see `register_fsp_channel`).
//! - `unix-mock` — Unix-domain-socket pair simulating one IVC channel.
//!   Linux-only, pulls `std`. Used by `cargo test`, by the
//!   `autoware_sentinel` POSIX dev path, and by the CCPLEX-side IVC
//!   bridge daemon's test fixtures.
//!
//! The same safe Rust API ([`Channel`], [`RxFrame`], [`TxFrame`]) and
//! the same C ABI (`nvidia_ivc_channel_*`) is exposed by both backends —
//! callers do not branch on which one is active.
//!
//! # Zero-copy contract (Phase 11.3.A)
//!
//! NVIDIA's FSP IVC API is fundamentally **zero-copy**: the channel
//! library hands the caller a pointer into the shared-memory ring
//! buffer (an "RX frame" or a "TX slot"); the caller fills/reads it
//! in place; a separate "release" or "commit" call advances the ring
//! cursor. We mirror that on both backends:
//!
//! - [`Channel::read_frame`] returns `Some(RxFrame<'_>)` if a frame
//!   is available; the borrow lasts until the `RxFrame` is dropped or
//!   [`RxFrame::ack`]'d, which releases the slot back to the producer.
//! - [`Channel::write_frame`] returns `Some(TxFrame<'_>)` if a TX slot
//!   is available; the caller fills [`TxFrame::as_mut_slice`], then
//!   commits via [`TxFrame::commit`]. Dropping a `TxFrame` without
//!   committing abandons the in-flight slot (leaves it free for the
//!   next call; nothing is sent).
//! - [`Channel::notify`] rings the doorbell so the peer wakes and
//!   processes whatever was just committed. Batch multiple commits
//!   under one `notify` to amortise the IRQ cost.
//!
//! # Layering
//!
//! This crate is **self-contained**: it has no dep on `nros-platform`,
//! `nros-rmw`, or zenoh-pico. Reusable by any project that needs a
//! Tegra Cortex-R5/R52 IVC driver. Higher layers wire it in:
//!
//! - `nros-platform-orin-spe` implements `PlatformIvc` by delegating
//!   here.
//! - `zpico-platform-shim::ivc_helpers` re-exports the `extern "C"`
//!   wrappers under the `_z_ivc_*` symbol names zenoh-pico's link-IVC C
//!   code expects.

#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::missing_safety_doc)]

#[cfg(all(feature = "fsp", feature = "unix-mock"))]
compile_error!(
    "nvidia-ivc: features `fsp` and `unix-mock` are mutually exclusive — pick one"
);

mod error;
pub use error::IvcError;

#[cfg(feature = "fsp")]
pub mod fsp;
#[cfg(feature = "fsp")]
use fsp as backend;

#[cfg(feature = "unix-mock")]
pub mod unix_mock;
#[cfg(feature = "unix-mock")]
use unix_mock as backend;

// Stub backend used when no feature is selected. Lets the crate
// `cargo build` without features so docs / metadata / `cargo check`
// are clean from a fresh tree; every entry point fails closed.
#[cfg(not(any(feature = "fsp", feature = "unix-mock")))]
mod stub;
#[cfg(not(any(feature = "fsp", feature = "unix-mock")))]
use stub as backend;

use core::ffi::c_void;

/// Opaque handle to one IVC channel.
///
/// Cheaply [`Clone`] / [`Copy`] because the underlying state is owned
/// by the backend (FSP keeps it inside `tegra_aon_fsp.a` plus a
/// caller-supplied `tegra_ivc_channel` struct; the `unix-mock` backend
/// keeps it in a `static` table). Dropping a `Channel` is a no-op —
/// the channel lifetime equals the carveout lifetime, which is the
/// program lifetime in practice.
#[derive(Clone, Copy)]
pub struct Channel(pub(crate) *mut c_void);

// SAFETY: backends serialise internally where required. Channel
// handles are pointer-shaped wrappers; sharing one across threads is
// no different from sharing the underlying SPSC ring.
unsafe impl Send for Channel {}
unsafe impl Sync for Channel {}

impl Channel {
    /// Resolve a channel ID into a handle, or `None` if the carveout
    /// does not advertise that ID.
    pub fn open(id: u32) -> Option<Self> {
        let raw = backend::channel_get(id);
        if raw.is_null() { None } else { Some(Self(raw)) }
    }

    /// Per-channel frame size, in bytes (typical NVIDIA IVC: 64).
    pub fn frame_size(&self) -> usize {
        unsafe { backend::frame_size(self.0) as usize }
    }

    /// Borrow the next-available RX frame. Returns `None` if the ring
    /// is empty.
    ///
    /// The returned [`RxFrame`] borrows the channel; only one frame
    /// may be outstanding per `Channel` at a time. The borrow is
    /// released when the `RxFrame` is dropped or [`RxFrame::ack`]'d,
    /// which advances the producer-visible cursor.
    pub fn read_frame(&self) -> Option<RxFrame<'_>> {
        let mut len: usize = 0;
        let ptr = unsafe { backend::rx_get(self.0, &mut len) };
        if ptr.is_null() {
            None
        } else {
            Some(RxFrame { ch: self, ptr, len })
        }
    }

    /// Borrow the next free TX slot. Returns `None` if the ring is
    /// full (peer hasn't drained yet).
    ///
    /// Fill via [`TxFrame::as_mut_slice`] then commit via
    /// [`TxFrame::commit`]. Dropping the `TxFrame` without committing
    /// abandons the slot — nothing is sent and the slot remains free
    /// for the next call.
    pub fn write_frame(&self) -> Option<TxFrame<'_>> {
        let mut capacity: usize = 0;
        let ptr = unsafe { backend::tx_get(self.0, &mut capacity) };
        if ptr.is_null() {
            None
        } else {
            Some(TxFrame { ch: self, ptr, capacity, committed: false })
        }
    }

    /// Ring the doorbell so the peer wakes and processes the queue.
    /// Batch multiple `commit`s under one `notify`.
    pub fn notify(&self) {
        unsafe { backend::notify(self.0) }
    }
}

/// Borrowed RX frame. Drop or [`Self::ack`] to release the slot.
pub struct RxFrame<'a> {
    ch: &'a Channel,
    ptr: *const u8,
    len: usize,
}

impl<'a> RxFrame<'a> {
    /// Bytes in this frame.
    pub fn as_slice(&self) -> &[u8] {
        // SAFETY: the backend guarantees `ptr..ptr+len` is a valid
        // initialised IVC frame for the lifetime of this borrow (i.e.
        // until release).
        unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
    }

    /// Release the slot back to the producer. Equivalent to letting
    /// Drop run; provided so callers can be explicit at the call site.
    pub fn ack(self) {
        // Drop runs naturally on this scope exit.
        drop(self);
    }
}

impl<'a> Drop for RxFrame<'a> {
    fn drop(&mut self) {
        unsafe { backend::rx_release(self.ch.0) }
    }
}

/// Borrowed TX slot. Fill, then [`Self::commit`] to send. Drop without
/// commit abandons the slot.
pub struct TxFrame<'a> {
    ch: &'a Channel,
    ptr: *mut u8,
    capacity: usize,
    committed: bool,
}

impl<'a> TxFrame<'a> {
    /// Slot capacity (== `Channel::frame_size`).
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Mutable view over the slot. Caller writes the payload here.
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        // SAFETY: the backend guarantees `ptr..ptr+capacity` is a
        // valid mutable IVC frame for the lifetime of this borrow.
        unsafe { core::slice::from_raw_parts_mut(self.ptr, self.capacity) }
    }

    /// Commit `len` bytes (must be ≤ `capacity()`). Slot is then
    /// visible to the peer. Doorbell is **not** rung — call
    /// [`Channel::notify`] separately so writers can batch.
    pub fn commit(mut self, len: usize) {
        assert!(len <= self.capacity, "TxFrame::commit len {len} > capacity {}", self.capacity);
        unsafe { backend::tx_commit(self.ch.0, len) };
        self.committed = true;
    }
}

impl<'a> Drop for TxFrame<'a> {
    fn drop(&mut self) {
        if !self.committed {
            unsafe { backend::tx_abandon(self.ch.0) };
        }
    }
}

// =============================================================================
// C ABI — consumed by zenoh-pico's Z_FEATURE_LINK_IVC C code via the shim.
//
// Names match the prefix expected by the shim forwarders
// (`_z_open_ivc → nvidia_ivc_channel_get`, etc.).
// =============================================================================

/// Returns the opaque channel pointer, or null if `id` is not registered.
#[unsafe(no_mangle)]
pub extern "C" fn nvidia_ivc_channel_get(id: u32) -> *mut c_void {
    backend::channel_get(id)
}

/// Per-channel frame size, in bytes.
///
/// # Safety
///
/// `ch` must be a value previously returned by
/// `nvidia_ivc_channel_get` (or null, in which case 0 is returned).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nvidia_ivc_channel_frame_size(ch: *mut c_void) -> u32 {
    if ch.is_null() {
        0
    } else {
        unsafe { backend::frame_size(ch) }
    }
}

/// Borrow the next-available RX frame.
///
/// Writes the frame length to `*len_out` and returns a pointer into
/// the ring's shared memory; the data is valid until
/// `nvidia_ivc_channel_rx_release` is called. Returns null (and
/// `*len_out = 0`) if no frame is available.
///
/// # Safety
///
/// `ch` must be from `nvidia_ivc_channel_get`. `len_out` must be a
/// valid `*mut usize`. Caller must release exactly once via
/// `nvidia_ivc_channel_rx_release` before requesting the next frame.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nvidia_ivc_channel_rx_get(
    ch: *mut c_void,
    len_out: *mut usize,
) -> *const u8 {
    if ch.is_null() || len_out.is_null() {
        if !len_out.is_null() {
            unsafe { *len_out = 0 };
        }
        return core::ptr::null();
    }
    unsafe { backend::rx_get(ch, len_out) }
}

/// Release the most recently `rx_get`'d frame back to the producer.
///
/// # Safety
///
/// `ch` must be from `nvidia_ivc_channel_get`. Must be paired 1:1
/// with `nvidia_ivc_channel_rx_get` calls that returned non-null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nvidia_ivc_channel_rx_release(ch: *mut c_void) {
    if !ch.is_null() {
        unsafe { backend::rx_release(ch) }
    }
}

/// Borrow the next free TX slot.
///
/// Writes the slot capacity to `*cap_out` and returns a writable
/// pointer into the ring's shared memory. Returns null (and
/// `*cap_out = 0`) if the ring is full.
///
/// # Safety
///
/// `ch` must be from `nvidia_ivc_channel_get`. `cap_out` must be a
/// valid `*mut usize`. Caller must either commit (via
/// `nvidia_ivc_channel_tx_commit`) or abandon (via
/// `nvidia_ivc_channel_tx_abandon`) before requesting the next slot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nvidia_ivc_channel_tx_get(
    ch: *mut c_void,
    cap_out: *mut usize,
) -> *mut u8 {
    if ch.is_null() || cap_out.is_null() {
        if !cap_out.is_null() {
            unsafe { *cap_out = 0 };
        }
        return core::ptr::null_mut();
    }
    unsafe { backend::tx_get(ch, cap_out) }
}

/// Commit `len` bytes from the most recently `tx_get`'d slot. Slot is
/// then visible to the peer; doorbell is NOT rung — call
/// `nvidia_ivc_channel_notify` separately.
///
/// # Safety
///
/// `ch` must be from `nvidia_ivc_channel_get`. Must be paired 1:1
/// with `nvidia_ivc_channel_tx_get` calls that returned non-null.
/// `len` must be ≤ the capacity reported by `tx_get`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nvidia_ivc_channel_tx_commit(ch: *mut c_void, len: usize) {
    if !ch.is_null() {
        unsafe { backend::tx_commit(ch, len) }
    }
}

/// Abandon the most recently `tx_get`'d slot without sending.
///
/// # Safety
///
/// `ch` must be from `nvidia_ivc_channel_get`. Must be paired 1:1
/// with a `nvidia_ivc_channel_tx_get` call that returned non-null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nvidia_ivc_channel_tx_abandon(ch: *mut c_void) {
    if !ch.is_null() {
        unsafe { backend::tx_abandon(ch) }
    }
}

/// Doorbell — wake the peer.
///
/// # Safety
///
/// `ch` must be from `nvidia_ivc_channel_get`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nvidia_ivc_channel_notify(ch: *mut c_void) {
    if !ch.is_null() {
        unsafe { backend::notify(ch) }
    }
}
