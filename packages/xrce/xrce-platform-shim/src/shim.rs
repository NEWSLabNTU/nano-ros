use nros_platform::ConcretePlatform;

type P = ConcretePlatform;

// ============================================================================
// Clock — uxr_millis / uxr_nanos
// ============================================================================

/// Monotonic millisecond clock for XRCE-DDS session timeouts.
#[unsafe(no_mangle)]
pub extern "C" fn uxr_millis() -> i64 {
    P::clock_ms() as i64
}

/// Monotonic nanosecond clock for XRCE-DDS time synchronization.
#[unsafe(no_mangle)]
pub extern "C" fn uxr_nanos() -> i64 {
    P::clock_us() as i64 * 1000
}

// ============================================================================
// smoltcp clock (bare-metal networking only)
// ============================================================================

/// Monotonic clock for smoltcp TCP/IP timestamping.
///
/// Shared symbol used by both `zpico-smoltcp` and `xrce-smoltcp`.
#[cfg(feature = "smoltcp")]
#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_clock_now_ms() -> u64 {
    P::clock_ms()
}
