//! Sleep functions for zenoh-pico (busy-wait with network polling)
//!
//! Implements `z_sleep_us`, `z_sleep_ms`, `z_sleep_s`.
//! During sleep, the network stack is polled to avoid missing packets.

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
    // Busy-wait while polling smoltcp to avoid missing packets
    let start = clock::clock_ms();
    while clock::clock_ms().wrapping_sub(start) < time_ms as u64 {
        // smoltcp_poll is provided by the transport crate
        zpico_smoltcp::smoltcp_poll();
    }
    0 // _Z_RES_OK
}

/// z_result_t z_sleep_s(size_t time_s)
#[unsafe(no_mangle)]
pub extern "C" fn z_sleep_s(time_s: usize) -> i8 {
    z_sleep_ms(time_s * 1000)
}
