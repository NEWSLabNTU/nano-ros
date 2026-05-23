//! FreeRTOS QEMU Listener (Phase 118 collapsed)
//!
//! Subscribes to `std_msgs/Int32` messages on `/chatter`. RMW selected
//! at build time via mutually exclusive `rmw-{zenoh,cyclonedds}` Cargo
//! features; source body stays RMW-agnostic.

#![no_std]
#![no_main]

#[unsafe(no_mangle)]
extern "C" fn _start() -> ! {
    qemu_freertos_listener::start_from_reset()
}
