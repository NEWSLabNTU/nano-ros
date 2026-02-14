//! QEMU Listener - ROS 2 Int32 Subscriber for MPS2-AN385
//!
//! Subscribes to typed `std_msgs/Int32` messages on `/chatter`,
//! compatible with ROS 2 nodes via rmw_zenoh.

#![no_std]
#![no_main]

use nros_mps2_an385::prelude::*;
use nros_mps2_an385::println;
use panic_semihosting as _;
use std_msgs::msg::Int32;

#[entry]
fn main() -> ! {
    // Use listener config (different IP/MAC than talker)
    run_node(Config::listener(), |node| {
        println!("Subscribing to /chatter (std_msgs/Int32)");

        let mut subscription = node.create_subscription::<Int32>("/chatter")?;

        println!("Subscriber declared");
        println!("");
        println!("Waiting for messages...");

        let mut msg_count = 0u32;
        let mut poll_count = 0u32;

        loop {
            node.spin_once(10);

            if let Some(msg) = subscription.try_recv()? {
                msg_count += 1;
                println!("Received [{}]: {}", msg_count, msg.data);

                if msg_count >= 10 {
                    println!("");
                    println!("Received 10 messages, exiting.");
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
