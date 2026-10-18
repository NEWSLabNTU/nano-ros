//! phase-397 W2 (RFC-0084) — an arrival names its ENTITY and its OWNING
//! executor.
//!
//! The session-scoped wake (`set_wake_callback`) carries only the executor's
//! context, so an arrival is binary: "something happened on this session". Two
//! things are lost at that ABI. The executor cannot tell which entry is ready,
//! so it scans them all. And when several executors share one session (the
//! wake fan-out, RFC-0106 D6), every arrival wakes every one of them, at a
//! measured 7-9 us of CPU per spurious wake (phase-436 step 5).
//!
//! Each subscription, service and client entry is registered with a [`Waker`]
//! from [`waker`], through the handle's `register_waker` (W1 carries it across
//! the C ABI as upstream's `*_set_on_new_*_callback`). That waker identifies:
//!
//! - the owning executor's [`WakeCtx`], an `Arc` the waker holds a strong
//!   reference to, so a backend that keeps a clone past the entry's release
//!   cannot dangle;
//! - the entry's slot, in the low six bits of the same pointer. `WakeCtx` is
//!   `repr(align(64))`, so those bits are free.
//!
//! Waking it sets that slot's bit in [`WakeCtx::ready`] and then does exactly
//! what the session callback does: flag, probe, signal. A backend that
//! attributes the arrival this way can skip the session wake for it, so only
//! the owner wakes. A backend that cannot attribute (NULL slot, or
//! `UNSUPPORTED`) keeps the session wake, and the fan-out stays the fallback.
//!
//! W3 is the consumer of `ready`. Until it lands, the executor still scans;
//! the bits are accurate but unread.

use core::task::{RawWaker, RawWakerVTable, Waker};

use portable_atomic_util::Arc;

use super::spin::WakeCtx;

/// Slots the ready word can name: one `u64`. An entry above this keeps the
/// session-scoped wake.
pub(crate) const SLOTS: usize = 64;

const SLOT_MASK: usize = SLOTS - 1;

const _: () = assert!(
    core::mem::align_of::<WakeCtx>() >= SLOTS,
    "the slot rides in the low bits of the WakeCtx pointer, so WakeCtx must be aligned to SLOTS"
);

/// The waker for `slot` of the executor that owns `ctx`, or `None` when the
/// slot is beyond what the ready word can name.
pub(crate) fn waker(ctx: &Arc<WakeCtx>, slot: usize) -> Option<Waker> {
    if slot >= SLOTS {
        return None;
    }
    let base = Arc::into_raw(ctx.clone()) as usize;
    debug_assert_eq!(base & SLOT_MASK, 0, "WakeCtx is not aligned to SLOTS");
    let raw = RawWaker::new((base | slot) as *const (), &VTABLE);
    // SAFETY: the vtable honours the RawWaker contract: `clone` takes a strong
    // reference, `drop` releases one, and `wake` consumes the one it was given.
    Some(unsafe { Waker::from_raw(raw) })
}

fn split(data: *const ()) -> (*const WakeCtx, usize) {
    let d = data as usize;
    ((d & !SLOT_MASK) as *const WakeCtx, d & SLOT_MASK)
}

static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake_by_ref, drop);

unsafe fn clone(data: *const ()) -> RawWaker {
    // SAFETY: `data` came from `waker()`, so its base is an `Arc::into_raw`
    // pointer whose count this waker holds one of.
    unsafe { Arc::increment_strong_count(split(data).0) };
    RawWaker::new(data, &VTABLE)
}

unsafe fn wake(data: *const ()) {
    unsafe {
        wake_by_ref(data);
        drop(data);
    }
}

unsafe fn wake_by_ref(data: *const ()) {
    let (ctx, slot) = split(data);
    // SAFETY: this waker holds a strong reference, so the `WakeCtx` is alive.
    unsafe { &*ctx }.mark_ready(slot);
}

unsafe fn drop(data: *const ()) {
    // SAFETY: releases the strong reference this waker held.
    unsafe { Arc::decrement_strong_count(split(data).0) };
}
