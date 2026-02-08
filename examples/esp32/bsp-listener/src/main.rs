//! Simple ESP32 WiFi Listener using nano-ros-bsp-esp32
//!
//! Subscribes to typed `std_msgs/Int32` messages on `/chatter`.
//! Compare with the QEMU bsp-listener to see the similar API.
//!
//! # Building
//!
//! ```bash
//! SSID=MyNetwork PASSWORD=secret cargo +nightly build --release
//! ```
//!
//! # Flashing
//!
//! ```bash
//! SSID=MyNetwork PASSWORD=secret cargo +nightly run --release
//! ```

#![no_std]
#![no_main]

use esp_backtrace as _;
use nano_ros_bsp_esp32::esp_println;
use nano_ros_bsp_esp32::portable_atomic::{AtomicU32, Ordering};
use nano_ros_bsp_esp32::prelude::*;
use std_msgs::msg::Int32;

/// WiFi credentials (set via environment variables at compile time)
const SSID: &str = env!("SSID");
const PASSWORD: &str = env!("PASSWORD");

/// Last received Int32 value
static mut LAST_VALUE: i32 = 0;

/// Message count (portable-atomic provides safe atomics on riscv32imc)
static MSG_COUNT: AtomicU32 = AtomicU32::new(0);

/// Typed subscriber callback
fn on_message(msg: &Int32) {
    unsafe {
        LAST_VALUE = msg.data;
    }
    MSG_COUNT.fetch_add(1, Ordering::Relaxed);
}

nano_ros_bsp_esp32::esp_bootloader_esp_idf::esp_app_desc!();

#[entry]
fn main() -> ! {
    run_node(
        NodeConfig::new(WifiConfig::new(SSID, PASSWORD)),
        |node| {
            // Declare subscription
            esp_println::println!("Subscribing to /chatter (std_msgs/Int32)");

            let _subscription = node.create_subscription::<Int32>("/chatter", on_message)?;

            esp_println::println!("Subscriber declared");
            esp_println::println!("");
            esp_println::println!("Waiting for messages...");

            // Receive messages
            let mut last_count = 0u32;
            let mut poll_count = 0u32;

            loop {
                // Poll to process network events
                node.spin_once(10);

                // Check for new messages
                let current_count = MSG_COUNT.load(Ordering::Relaxed);
                if current_count > last_count {
                    let value = unsafe { LAST_VALUE };
                    esp_println::println!("Received [{}]: {}", current_count, value);
                    last_count = current_count;

                    // Exit after receiving 10 messages
                    if current_count >= 10 {
                        esp_println::println!("");
                        esp_println::println!("Received 10 messages.");
                        break;
                    }
                }

                poll_count += 1;

                // Safety timeout (much longer than QEMU since WiFi is slower)
                if poll_count > 1_000_000 {
                    esp_println::println!("");
                    esp_println::println!("Timeout waiting for messages.");
                    break;
                }
            }

            Ok(())
        },
    )
}
