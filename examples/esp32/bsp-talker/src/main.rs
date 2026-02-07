//! Simple ESP32 WiFi Talker using nano-ros-bsp-esp32
//!
//! This example connects to WiFi, establishes a zenoh session, and publishes
//! messages. Compare with the QEMU bsp-talker to see the similar API.
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
use nano_ros_bsp_esp32::prelude::*;

/// WiFi credentials (set via environment variables at compile time)
const SSID: &str = env!("SSID");
const PASSWORD: &str = env!("PASSWORD");

/// Topic to publish on
const TOPIC: &[u8] = b"demo/esp32\0";

nano_ros_bsp_esp32::esp_bootloader_esp_idf::esp_app_desc!();

#[entry]
fn main() -> ! {
    run_node(
        NodeConfig::new(WifiConfig::new(SSID, PASSWORD)),
        |node| {
            // Declare publisher
            esp_println::println!("Declaring publisher on topic: demo/esp32");
            let publisher = node.create_publisher(TOPIC)?;
            esp_println::println!("Publisher declared (handle: {})", publisher.handle());

            // Publish messages
            esp_println::println!("");
            esp_println::println!("Publishing messages...");

            let mut msg_buf = [0u8; 64];

            for i in 0..10 {
                // Poll to process network events
                for _ in 0..100 {
                    node.spin_once(10);
                }

                // Format and publish message
                let msg = format_message(&mut msg_buf, i);

                if let Err(e) = publisher.publish(msg) {
                    esp_println::println!("Publish failed: {:?}", e);
                } else {
                    esp_println::println!(
                        "Published: {}",
                        core::str::from_utf8(msg).unwrap_or("?")
                    );
                }
            }

            esp_println::println!("");
            esp_println::println!("Done publishing 10 messages.");

            Ok(())
        },
    )
}

/// Format a message into the buffer
fn format_message(buf: &mut [u8], num: u32) -> &[u8] {
    let prefix = b"Hello from ESP32 #";
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
