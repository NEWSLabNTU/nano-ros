//! nano-ros STM32F4 Talker Example using BSP
//!
//! This example demonstrates the simplified BSP API that hides all
//! platform-specific details (clocks, GPIO, Ethernet, smoltcp).
//!
//! # Hardware
//!
//! - Board: NUCLEO-F429ZI (or similar STM32F4 with Ethernet)
//! - Connect Ethernet cable to the board's RJ45 port
//!
//! # Network Configuration
//!
//! Default (static IP):
//! - Device IP: 192.168.1.10/24
//! - Gateway: 192.168.1.1
//! - Zenoh Router: 192.168.1.1:7447
//!
//! # Building
//!
//! ```bash
//! cargo build --release
//! cargo run --release  # Uses probe-rs to flash
//! ```

#![no_std]
#![no_main]

use defmt_rtt as _;
use panic_probe as _;

use nano_ros_bsp_stm32f4::prelude::*;

/// ROS 2 keyexpr for std_msgs/Int32 on /chatter topic
const TOPIC: &[u8] = b"0/chatter/std_msgs::msg::dds_::Int32_/TypeHashNotSupported\0";

/// Poll interval in milliseconds
const POLL_INTERVAL_MS: u32 = 10;

/// Publish interval in milliseconds
const PUBLISH_INTERVAL_MS: u32 = 1000;

#[entry]
fn main() -> ! {
    run_node(Config::nucleo_f429zi(), |node| {
        info!("Creating publisher for /chatter...");
        let publisher = node.create_publisher(TOPIC)?;

        info!("Starting publish loop (1 Hz)...");
        let mut counter: i32 = 0;
        let mut last_publish_ms: u64 = 0;

        loop {
            // Poll network and process callbacks
            node.spin_once(POLL_INTERVAL_MS);

            // Check if it's time to publish
            let now_ms = node.now_ms();
            if now_ms - last_publish_ms >= PUBLISH_INTERVAL_MS as u64 {
                last_publish_ms = now_ms;
                counter = counter.wrapping_add(1);

                // Create CDR-encoded Int32 message
                // CDR format: 4-byte header + 4-byte int32
                let mut cdr_buffer = [0u8; 8];
                cdr_buffer[0] = 0x00; // CDR header: little-endian, no options
                cdr_buffer[1] = 0x01;
                cdr_buffer[2] = 0x00;
                cdr_buffer[3] = 0x00;
                // Little-endian int32
                cdr_buffer[4..8].copy_from_slice(&counter.to_le_bytes());

                match publisher.publish(&cdr_buffer) {
                    Ok(()) => info!("Published: {}", counter),
                    Err(e) => warn!("Publish failed: {:?}", e),
                }
            }
        }
    })
}
