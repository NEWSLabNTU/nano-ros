//! phase-521 W5 (RFC-0113 D5) — the executor's wake object on bare metal.
//!
//! There is no kernel to block on. A wake object is one atomic flag: a signal
//! sets it (from the main loop or an ISR), and a park waits for it on `wfi`
//! through the board's idle primitive, which sleeps until the deadline or until
//! the transport has received data (the LAN9118 or UART receive interrupt plus
//! a SysTick one-shot, `nros-board-mps2-an385`'s `tickless`).
//!
//! A park also ends when the board reports received data, so an arrival on the
//! link ends the executor's park even though nothing in the RX interrupt knows
//! which executor to signal: on this single-core board there is one executor
//! loop, and the data is drained by its next non-blocking `drive_io`.
//!
//! Until a board installs its idle primitive ([`set_wake_idle`]) a wake object
//! cannot be made (`wake_init` fails), so the executor keeps the
//! receive-blocking wait it always had.

use core::sync::atomic::{AtomicBool, AtomicPtr, Ordering};

/// The board's idle primitive: sleep until `deadline_ns` (on
/// [`crate::clock::clock_ns`]) or until the transport has received data, and
/// return whether it has. It may return early for any other reason.
pub type WakeIdleFn = unsafe extern "C" fn(deadline_ns: u64) -> bool;

static WAKE_IDLE: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());

/// Install the board's [`WakeIdleFn`]. Wake objects made before this fail.
pub fn set_wake_idle(f: WakeIdleFn) {
    WAKE_IDLE.store(f as *mut (), Ordering::Release);
}

fn wake_idle() -> Option<WakeIdleFn> {
    let f = WAKE_IDLE.load(Ordering::Acquire);
    // SAFETY: stored from a `WakeIdleFn` by `set_wake_idle`.
    (!f.is_null()).then(|| unsafe { core::mem::transmute::<*mut (), WakeIdleFn>(f) })
}

fn flag<'a>(w: *mut core::ffi::c_void) -> Option<&'a AtomicBool> {
    // SAFETY: `w` is caller storage of `storage_size` bytes and `storage_align`
    // alignment, initialised by `init` (the wake ABI's contract).
    (!w.is_null()).then(|| unsafe { &*(w as *const AtomicBool) })
}

pub(crate) const fn storage_size() -> usize {
    core::mem::size_of::<AtomicBool>()
}

pub(crate) const fn storage_align() -> usize {
    core::mem::align_of::<AtomicBool>()
}

pub(crate) fn init(w: *mut core::ffi::c_void) -> i8 {
    if w.is_null() || wake_idle().is_none() {
        return -1;
    }
    // SAFETY: as `flag`; this is the write that initialises it.
    unsafe { (w as *mut AtomicBool).write(AtomicBool::new(false)) };
    0
}

pub(crate) fn signal(w: *mut core::ffi::c_void) -> i8 {
    match flag(w) {
        Some(f) => {
            f.store(true, Ordering::Release);
            0
        }
        None => -1,
    }
}

/// Wait until signalled (0), the deadline passes (1), or the board's idle
/// reports received data (0: the drive after the park takes it).
pub(crate) fn park_until_us(w: *mut core::ffi::c_void, rel_us: u64) -> i8 {
    let (Some(f), Some(idle)) = (flag(w), wake_idle()) else {
        return -1;
    };
    let deadline = crate::clock::clock_ns().saturating_add(rel_us.saturating_mul(1000));
    loop {
        if f.swap(false, Ordering::AcqRel) {
            return 0;
        }
        if crate::clock::clock_ns() >= deadline {
            return 1;
        }
        // SAFETY: the board's primitive, installed by `set_wake_idle`.
        if unsafe { idle(deadline) } {
            return 0;
        }
    }
}
