//! Busy-wait sleep — delegates to `nros_baremetal_common::sleep`.

pub use nros_baremetal_common::sleep::{
    PollFn, clear_poll_callback, set_poll_callback, sleep_ms,
};

/// Register the platform's clock function with the shared sleep
/// module. Must be called once at platform init before any
/// `sleep_ms` call.
pub fn init_clock() {
    nros_baremetal_common::sleep::set_clock_fn(crate::clock::clock_ms);
}
