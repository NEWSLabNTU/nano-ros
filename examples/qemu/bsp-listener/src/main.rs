//! Simple QEMU Listener using nano-ros-bsp-qemu
//!
//! Subscribes to typed `std_msgs/Int32` messages on `/chatter`.
//! Compare with qemu-rs-listener to see the reduced boilerplate.

#![no_std]
#![no_main]

use core::sync::atomic::{AtomicI32, AtomicU32, Ordering};

use nano_ros_bsp_qemu::prelude::*;
use nano_ros_bsp_qemu::println;
use panic_semihosting as _;
use std_msgs::msg::Int32;

/// Last received Int32 value (atomic for safe callback access)
static LAST_VALUE: AtomicI32 = AtomicI32::new(0);

/// Message count (atomic for safe callback access)
static MSG_COUNT: AtomicU32 = AtomicU32::new(0);

/// Typed subscriber callback
fn on_message(msg: &Int32) {
    LAST_VALUE.store(msg.data, Ordering::Relaxed);
    MSG_COUNT.fetch_add(1, Ordering::SeqCst);
}

#[entry]
fn main() -> ! {
    // Use listener config (different IP/MAC than talker)
    run_node(Config::listener(), |node| {
        println!("Subscribing to /chatter (std_msgs/Int32)");

        let _subscription = node.create_subscription::<Int32>("/chatter", on_message)?;

        println!("Subscriber declared");
        println!("");
        println!("Waiting for messages...");

        let mut last_count = 0u32;
        let mut poll_count = 0u32;

        loop {
            node.spin_once(10);

            let current_count = MSG_COUNT.load(Ordering::SeqCst);
            if current_count > last_count {
                let value = LAST_VALUE.load(Ordering::Relaxed);
                println!("Received [{}]: {}", current_count, value);
                last_count = current_count;

                if current_count >= 10 {
                    println!("");
                    println!("Received 10 messages.");
                    break;
                }
            }

            poll_count += 1;
            if poll_count > 100000 {
                println!("");
                println!("Timeout waiting for messages.");
                break;
            }
        }

        Ok(())
    })
}
