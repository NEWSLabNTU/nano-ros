//! Busy-wait sleep with optional poll callback for bare-metal platforms.
//!
//! Each platform's clock source is different (DWT cycle counter on ARM,
//! `esp_hal::time::Instant` on ESP32, etc.), so this module accepts a
//! clock function pointer registered at startup. The platform crate's
//! init code calls [`set_clock_fn`] with its own `clock_ms`
//! implementation; subsequent [`sleep_ms`] calls poll that function in
//! a busy-wait loop.
//!
//! During sleep, an optional poll callback (typically
//! `smoltcp_network_poll`) is invoked each iteration so network I/O
//! continues during the delay.

use core::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

/// Clock function type — returns monotonic milliseconds since boot.
pub type ClockMsFn = fn() -> u64;

/// Poll callback type — invoked during busy-wait sleep.
/// Uses `extern "C"` to match `smoltcp_network_poll` and similar callbacks.
pub type PollFn = unsafe extern "C" fn();

// We can't store `fn() -> u64` directly in an atomic because Rust's atomic
// types only accept pointer-sized values; we coerce to `usize` and back.
static CLOCK_FN: AtomicUsize = AtomicUsize::new(0);
static POLL_CALLBACK: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());

/// Register the platform's `clock_ms` function. Must be called once at
/// platform init before [`sleep_ms`] is invoked.
pub fn set_clock_fn(f: ClockMsFn) {
    CLOCK_FN.store(f as usize, Ordering::Release);
}

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
///
/// No-op if no clock function has been registered yet.
pub fn sleep_ms(time_ms: usize) {
    let clock_addr = CLOCK_FN.load(Ordering::Acquire);
    if clock_addr == 0 {
        return; // Clock not registered yet — silent no-op.
    }
    // SAFETY: The address is always set via `set_clock_fn` which takes a
    // `fn() -> u64`. The transmute round-trips that exact type.
    let clock: ClockMsFn = unsafe { core::mem::transmute(clock_addr) };

    let start = clock();
    while clock().wrapping_sub(start) < time_ms as u64 {
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
