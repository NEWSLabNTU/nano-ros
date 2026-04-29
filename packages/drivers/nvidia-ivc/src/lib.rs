//! NVIDIA Tegra IVC driver — `no_std`-friendly safe Rust API plus C-callable
//! `extern "C"` wrappers consumed by zenoh-pico's `Z_FEATURE_LINK_IVC` C
//! code.
//!
//! # Backends
//!
//! Exactly one backend feature must be enabled per build:
//!
//! - `fsp` — links NVIDIA's `tegra_aon_fsp.a` via the `NV_SPE_FSP_DIR`
//!   env var (see `build.rs`). `no_std`, `armv7r-none-eabihf`. Requires
//!   SDK Manager creds.
//! - `unix-mock` — Unix-domain-socket pair simulating one IVC channel.
//!   Linux-only, pulls `std`. Used by `cargo test`, by the
//!   `autoware_sentinel` POSIX dev path, and by the CCPLEX-side IVC
//!   bridge daemon's test fixtures.
//!
//! The same safe Rust API ([`Channel`]) and the same C ABI
//! (`nvidia_ivc_channel_*`) is exposed by both backends — callers do not
//! branch on which one is active.
//!
//! # Layering
//!
//! This crate is **self-contained**: it has no dep on `nros-platform`,
//! `nros-rmw`, or zenoh-pico. Reusable by any project that needs a Tegra
//! Cortex-R5/R52 IVC driver. Higher layers wire it in:
//!
//! - `nros-platform-orin-spe` implements `PlatformIvc` by delegating
//!   here.
//! - `zpico-platform-shim::ivc_helpers` re-exports the `extern "C"`
//!   wrappers under the `_z_*_ivc` symbol names zenoh-pico's link-IVC C
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
mod fsp;
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
/// Cheaply `Copy`/`Clone` because the underlying state is owned by the
/// backend (FSP keeps it inside `tegra_aon_fsp.a`; the `unix-mock`
/// backend keeps it in a `static` table). Dropping a `Channel` is a
/// no-op — the channel lifetime equals the carveout lifetime, which is
/// the program lifetime in practice.
#[derive(Clone, Copy)]
pub struct Channel(pub(crate) *mut c_void);

// Channels can be shared across threads — the underlying ring is SPSC
// per direction, but the safe API documents `&self` for read/write and
// the backends serialise internally where required.
unsafe impl Send for Channel {}
unsafe impl Sync for Channel {}

impl Channel {
    /// Resolve a channel ID into a handle, or `None` if the carveout
    /// does not advertise that ID.
    pub fn open(id: u32) -> Option<Self> {
        let raw = backend::channel_get(id);
        if raw.is_null() {
            None
        } else {
            Some(Self(raw))
        }
    }

    /// Read up to `buf.len()` bytes from the channel.
    ///
    /// Returns `Ok(n)` for `n` bytes read, `Err(WouldBlock)` if no
    /// frame is currently available, or `Err(Io)` on a hard transport
    /// error.
    pub fn read(&self, buf: &mut [u8]) -> Result<usize, IvcError> {
        let n = unsafe { backend::read(self.0, buf.as_mut_ptr(), buf.len()) };
        decode_io(n)
    }

    /// Write `buf.len()` bytes to the channel. Frame fragmentation is
    /// the caller's responsibility — `nvidia-ivc` does not split a
    /// large `buf` across multiple frames.
    pub fn write(&self, buf: &[u8]) -> Result<usize, IvcError> {
        let n = unsafe { backend::write(self.0, buf.as_ptr(), buf.len()) };
        decode_io(n)
    }

    /// Ring the doorbell so the peer wakes and processes the queue.
    pub fn notify(&self) {
        unsafe { backend::notify(self.0) }
    }

    /// Per-channel frame size, in bytes (typical NVIDIA IVC: 64).
    pub fn frame_size(&self) -> usize {
        unsafe { backend::frame_size(self.0) as usize }
    }
}

fn decode_io(n: usize) -> Result<usize, IvcError> {
    if n == usize::MAX {
        Err(IvcError::Io)
    } else if n == 0 {
        Err(IvcError::WouldBlock)
    } else {
        Ok(n)
    }
}

// =============================================================================
// C ABI — consumed by zenoh-pico's Z_FEATURE_LINK_IVC C code via the shim.
// Names match the prefix expected by the shim forwarders
// (`_z_open_ivc → nvidia_ivc_channel_get` / read / write / notify).
// =============================================================================

/// Returns the opaque channel pointer, or null if `id` is not registered.
#[unsafe(no_mangle)]
pub extern "C" fn nvidia_ivc_channel_get(id: u32) -> *mut c_void {
    backend::channel_get(id)
}

/// Read at most `len` bytes. Returns bytes read (zero == no frame),
/// or `usize::MAX` on hard error.
///
/// # Safety
///
/// `buf` must be valid for writes of `len` bytes; `ch` must be a value
/// previously returned by `nvidia_ivc_channel_get`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nvidia_ivc_channel_read(
    ch: *mut c_void,
    buf: *mut u8,
    len: usize,
) -> usize {
    unsafe { backend::read(ch, buf, len) }
}

/// Write `len` bytes. Returns bytes written, or `usize::MAX` on error.
///
/// # Safety
///
/// `buf` must be valid for reads of `len` bytes; `ch` must be a value
/// previously returned by `nvidia_ivc_channel_get`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nvidia_ivc_channel_write(
    ch: *mut c_void,
    buf: *const u8,
    len: usize,
) -> usize {
    unsafe { backend::write(ch, buf, len) }
}

/// Doorbell.
///
/// # Safety
///
/// `ch` must be a value previously returned by
/// `nvidia_ivc_channel_get`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nvidia_ivc_channel_notify(ch: *mut c_void) {
    unsafe { backend::notify(ch) }
}

/// Per-channel frame size, in bytes.
///
/// # Safety
///
/// `ch` must be a value previously returned by
/// `nvidia_ivc_channel_get`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nvidia_ivc_channel_frame_size(ch: *mut c_void) -> u32 {
    unsafe { backend::frame_size(ch) }
}
