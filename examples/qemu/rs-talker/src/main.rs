//! QEMU Talker - ROS 2 Int32 Publisher for MPS2-AN385
//!
//! Publishes typed `std_msgs/Int32` messages on `/chatter`,
//! compatible with ROS 2 nodes via rmw_zenoh.

#![no_std]
#![no_main]

use nros_qemu::prelude::*;
use nros_qemu::println;
use panic_semihosting as _;
use std_msgs::msg::Int32;

#[entry]
fn main() -> ! {
    run_node(Config::default(), |node| {
        println!("Declaring publisher on /chatter (std_msgs/Int32)");
        let publisher = node.create_publisher::<Int32>("/chatter")?;
        println!("Publisher declared");

        println!("");
        println!("Publishing messages...");

        for i in 0..10i32 {
            // Poll to process network events
            for _ in 0..100 {
                node.spin_once(10);
            }

            if let Err(e) = publisher.publish(&Int32 { data: i }) {
                println!("Publish failed: {:?}", e);
            } else {
                println!("Published: {}", i);
            }
        }

        println!("");
        println!("Done publishing 10 messages.");

        Ok(())
    })
}
