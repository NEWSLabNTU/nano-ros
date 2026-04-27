//! Wall-clock time for bare-metal MPS2-AN385.
//!
//! Without an RTC, delegates to the monotonic clock.
//! The `z_time_now_as_str` formatting is handled by the zpico shim crate,
//! not here — this module only provides raw time values.

// Time functions are implemented directly on Mps2An385Platform in lib.rs
// since they simply delegate to clock::clock_ms(). This module is reserved
// for any future time-specific logic (e.g., RTC integration).
