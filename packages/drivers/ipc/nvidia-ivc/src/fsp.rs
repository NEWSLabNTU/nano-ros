//! `fsp` backend — links NVIDIA's `tegra_aon_fsp.a` and forwards every
//! call to the matching `tegra_ivc_*` symbol from the FSP's
//! [`tegra-ivc.h`].
//!
//! NVIDIA's FSP IVC API is **zero-copy**: the channel library hands
//! the caller a pointer into the shared-memory ring (an "RX frame" or
//! a "TX slot"), the caller fills/reads it in place, and a separate
//! "release" / "commit" call advances the cursor (and triggers the
//! peer-doorbell callback the firmware registered in
//! `ch->notify_remote`).
//!
//! # Channel registration
//!
//! Unlike the `unix-mock` backend (which owns its own channel state),
//! FSP `tegra_ivc_channel` instances are **firmware-supplied**: the
//! per-board `ivc-channel-ids.c` (in NVIDIA's
//! `rt-aux-cpu-demo-fsp/platform/`) defines static channel structs
//! pointed at fixed `read_header` / `write_header` carveout offsets,
//! and `ivc_init_channels_ccplex()` initialises them. nano-ros doesn't
//! own any channel struct — it can't, because the carveout layout is
//! board-specific.
//!
//! Instead, the firmware crate (`sentinel-spe-firmware` on the
//! application side) calls [`register_fsp_channel`] once per ID after
//! the FSP-side init runs. Subsequent `Channel::open(id)` /
//! `nvidia_ivc_channel_get(id)` calls look up that registry.
//!
//! # Single-frame outstanding model
//!
//! FSP allows the caller to borrow many frames at once
//! (`tegra_ivc_rx_get_read_frame(ch, idx)` for any valid idx). nano-ros
//! borrows one at a time — it always asks for index 0 (oldest) and
//! releases exactly one via `tegra_ivc_rx_notify_buffers_consumed(ch,
//! 1)`. This matches the way zenoh-pico's link-layer
//! `__z_ivc_send_batch` / `__z_ivc_recv_batch` consume frames.

use core::{
    ffi::c_void,
    sync::atomic::{AtomicPtr, Ordering},
};

// =============================================================================
// FSP `tegra-ivc.h` C ABI — see SDK Manager-installed header for the
// authoritative documentation.
// =============================================================================

unsafe extern "C" {
    // Run-time queries / sync.
    fn tegra_ivc_channel_notified(ch: *mut c_void) -> bool;
    #[allow(dead_code)]
    fn tegra_ivc_channel_is_synchronized(ch: *mut c_void) -> bool;

    // RX path.
    fn tegra_ivc_rx_get_read_available(ch: *mut c_void) -> core::ffi::c_uint;
    fn tegra_ivc_rx_get_read_frame(ch: *mut c_void, n: core::ffi::c_uint) -> *const u8;
    fn tegra_ivc_rx_notify_buffers_consumed(
        ch: *mut c_void,
        count: core::ffi::c_int,
    ) -> core::ffi::c_int;

    // TX path.
    fn tegra_ivc_tx_get_write_space(ch: *mut c_void) -> core::ffi::c_uint;
    fn tegra_ivc_tx_get_write_buffer(ch: *mut c_void, n: core::ffi::c_uint) -> *mut u8;
    fn tegra_ivc_tx_send_buffers(ch: *mut c_void, count: core::ffi::c_int) -> core::ffi::c_int;
}

// `struct tegra_ivc_channel` field layout (from tegra-ivc.h). nano-ros
// only reads `frame_size` directly; the rest is the FSP's run-time
// state and we pass the struct pointer back through the C ABI for
// every call.
#[repr(C)]
#[allow(non_camel_case_types)]
#[allow(dead_code)]
struct tegra_ivc_channel {
    write_header: *mut c_void,
    read_header: *mut c_void,
    nframes: u32,
    frame_size: u32,
    notify_remote: *mut c_void, // function pointer
    channel_group: u32,
    write_count: u32,
    read_count: u32,
}

// =============================================================================
// Channel registry — populated by the firmware crate post-FSP-init.
// Lock-free single-writer/single-reader: the firmware crate writes
// during boot (before the IVC link layer starts), readers hit the
// table from the application task afterwards.
// =============================================================================

const MAX_CHANNELS: usize = 8;

struct Slot {
    id: u32,
    ptr: AtomicPtr<c_void>,
}

impl Slot {
    const EMPTY: Self = Self {
        id: u32::MAX,
        ptr: AtomicPtr::new(core::ptr::null_mut()),
    };
}

static REGISTRY: [Slot; MAX_CHANNELS] = [const { Slot::EMPTY }; MAX_CHANNELS];

// SAFETY: the only writers are during single-threaded boot
// (`register_fsp_channel`), readers see the post-init steady state.
// Atomics give us monotonic visibility without taking a lock that
// would need a critical_section impl from the firmware.
#[allow(dead_code)]
struct AssertSendSync;
unsafe impl Sync for Slot {}

/// Register a firmware-owned `struct tegra_ivc_channel *` under `id`
/// so that subsequent `Channel::open(id)` / `nvidia_ivc_channel_get(id)`
/// calls find it.
///
/// Call this once per channel during firmware boot, after the FSP's
/// `ivc_init_channels_ccplex()` runs (the channel has to be
/// fully initialised before nano-ros starts using it).
///
/// # Safety
///
/// `ch` must point to a fully-initialised `struct tegra_ivc_channel`
/// that lives for the entire program lifetime (typically a `static`
/// in the firmware's `ivc-channel-ids.c`). The pointed-to struct must
/// not move.
pub unsafe fn register_fsp_channel(id: u32, ch: *mut c_void) {
    // Find an empty slot or replace an existing entry with the same id.
    for slot in &REGISTRY {
        if slot.id == u32::MAX || slot.id == id {
            // Manual mutation through the const reference — REGISTRY
            // entries are interior-mutable via AtomicPtr, but the `id`
            // field is plain `u32`. Use a raw pointer write through
            // an UnsafeCell-equivalent.
            //
            // Since this is single-threaded boot code, the cleanest
            // shape is to store id in an AtomicU32, but for now we
            // require callers to register sequentially with no
            // contention.
            let id_ptr = (&slot.id as *const u32) as *mut u32;
            // SAFETY: single-threaded boot use only.
            unsafe { id_ptr.write(id) };
            slot.ptr.store(ch, Ordering::Release);
            return;
        }
    }
    panic!("nvidia-ivc fsp: no free slot for channel id {id} (max {MAX_CHANNELS})");
}

fn lookup(id: u32) -> *mut c_void {
    for slot in &REGISTRY {
        if slot.id == id {
            return slot.ptr.load(Ordering::Acquire);
        }
    }
    core::ptr::null_mut()
}

// =============================================================================
// Backend hooks called from `lib.rs`.
// =============================================================================

#[inline]
pub(crate) fn channel_get(id: u32) -> *mut c_void {
    lookup(id)
}

#[inline]
pub(crate) unsafe fn frame_size(ch: *mut c_void) -> u32 {
    if ch.is_null() {
        return 0;
    }
    let c = ch as *const tegra_ivc_channel;
    unsafe { (*c).frame_size }
}

#[inline]
pub(crate) unsafe fn rx_get(ch: *mut c_void, len_out: *mut usize) -> *const u8 {
    // Drain any pending sync state changes the peer raised. Cheap
    // when nothing changed; required after a fresh interrupt.
    let _ = unsafe { tegra_ivc_channel_notified(ch) };
    let avail = unsafe { tegra_ivc_rx_get_read_available(ch) };
    if avail == 0 {
        unsafe { *len_out = 0 };
        return core::ptr::null();
    }
    let ptr = unsafe { tegra_ivc_rx_get_read_frame(ch, 0) };
    if ptr.is_null() {
        unsafe { *len_out = 0 };
        return core::ptr::null();
    }
    let c = ch as *const tegra_ivc_channel;
    unsafe { *len_out = (*c).frame_size as usize };
    ptr
}

#[inline]
pub(crate) unsafe fn rx_release(ch: *mut c_void) {
    // Release exactly one frame. The wrapping notify_remote(true)
    // is invoked inside this call, so the peer learns it can refill.
    let _ = unsafe { tegra_ivc_rx_notify_buffers_consumed(ch, 1) };
}

#[inline]
pub(crate) unsafe fn tx_get(ch: *mut c_void, cap_out: *mut usize) -> *mut u8 {
    let _ = unsafe { tegra_ivc_channel_notified(ch) };
    let space = unsafe { tegra_ivc_tx_get_write_space(ch) };
    if space == 0 {
        unsafe { *cap_out = 0 };
        return core::ptr::null_mut();
    }
    let ptr = unsafe { tegra_ivc_tx_get_write_buffer(ch, 0) };
    if ptr.is_null() {
        unsafe { *cap_out = 0 };
        return core::ptr::null_mut();
    }
    let c = ch as *const tegra_ivc_channel;
    unsafe { *cap_out = (*c).frame_size as usize };
    ptr
}

#[inline]
pub(crate) unsafe fn tx_commit(ch: *mut c_void, _len: usize) {
    // FSP commits whole frames. The link-layer `len` is the payload
    // length the framing protocol cares about; below it, every IVC
    // frame is the channel's full `frame_size`. Commit one.
    let _ = unsafe { tegra_ivc_tx_send_buffers(ch, 1) };
}

#[inline]
pub(crate) unsafe fn tx_abandon(_ch: *mut c_void) {
    // FSP has no explicit abandon — the slot stays free for the next
    // `tegra_ivc_tx_get_write_buffer` call (we never advanced the
    // write cursor because we didn't call `tegra_ivc_tx_send_buffers`).
}

#[inline]
pub(crate) unsafe fn notify(_ch: *mut c_void) {
    // FSP's `tegra_ivc_tx_send_buffers` and
    // `tegra_ivc_rx_notify_buffers_consumed` already invoke
    // `ch->notify_remote(ch, ...)` internally. No additional doorbell
    // is needed; this hook is no-op for symmetry with the unix-mock
    // backend's `notify` (which is also no-op since SOCK_DGRAM wakes
    // the peer naturally).
}
