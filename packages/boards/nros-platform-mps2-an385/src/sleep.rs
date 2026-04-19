//! Busy-wait sleep — delegates to `nros_baremetal_common::sleep`.
//!
//! The platform's init code registers `clock::clock_ms` as the sleep
//! clock source via `nros_baremetal_common::sleep::set_clock_fn`.
//! Once that's done, calls to `sleep_ms` here forward to the shared
//! module which polls the registered clock in a busy-wait loop.

pub use nros_baremetal_common::sleep::{
    PollFn, clear_poll_callback, set_poll_callback, sleep_ms,
};

/// Register the platform's clock function with the shared sleep
/// module. Must be called once at platform init before any
/// `sleep_ms` call.
pub fn init_clock() {
    nros_baremetal_common::sleep::set_clock_fn(crate::clock::clock_ms);
}
