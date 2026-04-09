//! Sleep functions for bare-metal MPS2-AN385 (busy-wait with optional poll).
//!
//! During sleep, an optional poll callback is invoked each iteration.
//! The board crate registers the callback (e.g., smoltcp network poll)
//! during initialization.

use crate::clock;
use core::sync::atomic::{AtomicPtr, Ordering};

/// Poll callback type — called during busy-wait sleep.
type PollFn = unsafe fn();

/// Registered poll callback (set by board crate via `set_poll_callback`).
static POLL_CALLBACK: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());

/// Register a poll callback to be invoked during sleep.
///
/// Typically set to the smoltcp network poll function so packets
/// are processed during busy-wait delays.
pub fn set_poll_callback(callback: PollFn) {
    POLL_CALLBACK.store(callback as *mut (), Ordering::Release);
}

/// Clear the poll callback.
pub fn clear_poll_callback() {
    POLL_CALLBACK.store(core::ptr::null_mut(), Ordering::Release);
}

/// Busy-wait sleep with optional poll callback.
pub fn sleep_ms(time_ms: usize) {
    let start = clock::clock_ms();
    while clock::clock_ms().wrapping_sub(start) < time_ms as u64 {
        let cb = POLL_CALLBACK.load(Ordering::Acquire);
        if !cb.is_null() {
            unsafe {
                let f: PollFn = core::mem::transmute(cb);
                f();
            }
        } else {
            core::hint::spin_loop();
        }
    }
}
