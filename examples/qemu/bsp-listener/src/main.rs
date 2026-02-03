//! Simple QEMU Listener using nano-ros-bsp-qemu
//!
//! This example demonstrates the simplified BSP API for subscribers.
//! Compare with qemu-rs-listener to see the reduced boilerplate.

#![no_std]
#![no_main]

use core::ffi::c_void;
use core::ptr;
use core::sync::atomic::{AtomicU32, Ordering};

use nano_ros_bsp_qemu::prelude::*;
use nano_ros_bsp_qemu::println;
use panic_semihosting as _;

/// Topic to subscribe to
const TOPIC: &[u8] = b"demo/qemu\0";

/// Message buffer for storing received messages
const MSG_BUFFER_SIZE: usize = 256;
static mut MSG_BUFFER: [u8; MSG_BUFFER_SIZE] = [0u8; MSG_BUFFER_SIZE];
static mut MSG_LEN: usize = 0;

/// Message count (atomic for safe callback access)
static MSG_COUNT: AtomicU32 = AtomicU32::new(0);

/// Subscriber callback - called when a message is received
#[allow(static_mut_refs)]
extern "C" fn on_message(data: *const u8, len: usize, _ctx: *mut c_void) {
    // Copy message to buffer (using unsafe block for static mut access)
    unsafe {
        let copy_len = len.min(MSG_BUFFER_SIZE);
        ptr::copy_nonoverlapping(data, MSG_BUFFER.as_mut_ptr(), copy_len);
        MSG_LEN = copy_len;
    }

    // Increment message count
    MSG_COUNT.fetch_add(1, Ordering::SeqCst);
}

#[entry]
fn main() -> ! {
    // Use listener config (different IP/MAC than talker)
    run_node(Config::listener(), |node| {
        // Declare subscriber
        println!("Subscribing to topic: demo/qemu");

        let _subscriber =
            unsafe { node.create_subscriber(TOPIC, Some(on_message), ptr::null_mut()) }?;

        println!("Subscriber declared");
        println!("");
        println!("Waiting for messages...");

        // Receive messages
        let mut last_count = 0u32;
        let mut poll_count = 0u32;

        loop {
            // Poll to process network events
            node.spin_once(10);

            // Check for new messages
            let current_count = MSG_COUNT.load(Ordering::SeqCst);
            if current_count > last_count {
                // New message received
                #[allow(static_mut_refs)]
                unsafe {
                    let msg = &MSG_BUFFER[..MSG_LEN];
                    if let Ok(s) = core::str::from_utf8(msg) {
                        println!("Received [{}]: {}", current_count, s);
                    } else {
                        println!("Received [{}]: <{} bytes binary>", current_count, MSG_LEN);
                    }
                }
                last_count = current_count;

                // Exit after receiving 10 messages
                if current_count >= 10 {
                    println!("");
                    println!("Received 10 messages.");
                    break;
                }
            }

            poll_count += 1;

            // Safety timeout
            if poll_count > 100000 {
                println!("");
                println!("Timeout waiting for messages.");
                break;
            }
        }

        Ok(())
    })
}
