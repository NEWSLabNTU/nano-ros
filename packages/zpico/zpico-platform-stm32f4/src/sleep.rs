//! Sleep functions for zenoh-pico (busy-wait with network polling)
//!
//! Implements `z_sleep_us`, `z_sleep_ms`, `z_sleep_s`.
//! During sleep, the network stack is polled (if ethernet is enabled)
//! to avoid missing packets.

use crate::clock;

/// z_result_t z_sleep_us(size_t time_us)
#[unsafe(no_mangle)]
pub extern "C" fn z_sleep_us(time_us: usize) -> i8 {
    // Convert to milliseconds, rounding up
    let time_ms = time_us.div_ceil(1000);
    z_sleep_ms(time_ms)
}

/// z_result_t z_sleep_ms(size_t time_ms)
#[unsafe(no_mangle)]
pub extern "C" fn z_sleep_ms(time_ms: usize) -> i8 {
    let start = clock::clock_ms();
    while clock::clock_ms().wrapping_sub(start) < time_ms as u64 {
        // Poll smoltcp during busy-wait to avoid missing packets
        #[cfg(feature = "ethernet")]
        zpico_smoltcp::smoltcp_poll();

        #[cfg(not(feature = "ethernet"))]
        core::hint::spin_loop();
    }
    0 // _Z_RES_OK
}

/// z_result_t z_sleep_s(size_t time_s)
#[unsafe(no_mangle)]
pub extern "C" fn z_sleep_s(time_s: usize) -> i8 {
    z_sleep_ms(time_s * 1000)
}
