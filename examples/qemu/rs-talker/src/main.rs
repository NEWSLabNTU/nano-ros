//! QEMU Talker - zenoh-pico Publisher for MPS2-AN385
//!
//! This example uses nano-ros-bsp-qemu for simplified bare-metal setup.

#![no_std]
#![no_main]

use nano_ros_bsp_qemu::prelude::*;
use nano_ros_bsp_qemu::println;
use panic_semihosting as _;

/// Topic to publish on
const TOPIC: &[u8] = b"demo/qemu\0";

#[entry]
fn main() -> ! {
    run_node(Config::default(), |node| {
        // Declare publisher
        println!("Declaring publisher on topic: demo/qemu");
        let publisher = node.create_publisher(TOPIC)?;
        println!("Publisher declared (handle: {})", publisher.handle());

        // Publish messages
        println!("");
        println!("Publishing messages...");

        let mut msg_buf = [0u8; 64];

        for i in 0..10u32 {
            // Poll to process network events
            for _ in 0..100 {
                node.spin_once(10);
            }

            // Format and publish message
            let msg = format_message(&mut msg_buf, i);

            if let Err(e) = publisher.publish(msg) {
                println!("Publish failed: {:?}", e);
            } else {
                println!("Published: {}", core::str::from_utf8(msg).unwrap_or("?"));
            }
        }

        println!("");
        println!("Done publishing 10 messages.");

        Ok(())
    })
}

/// Format a message into the buffer
fn format_message(buf: &mut [u8], num: u32) -> &[u8] {
    let prefix = b"Hello from QEMU #";
    let mut pos = 0;

    // Copy prefix
    for &b in prefix {
        if pos < buf.len() {
            buf[pos] = b;
            pos += 1;
        }
    }

    // Convert number to string
    if num == 0 {
        if pos < buf.len() {
            buf[pos] = b'0';
            pos += 1;
        }
    } else {
        let mut n = num;
        let mut digits = [0u8; 10];
        let mut digit_count = 0;

        while n > 0 {
            digits[digit_count] = b'0' + (n % 10) as u8;
            n /= 10;
            digit_count += 1;
        }

        for i in (0..digit_count).rev() {
            if pos < buf.len() {
                buf[pos] = digits[i];
                pos += 1;
            }
        }
    }

    &buf[..pos]
}
