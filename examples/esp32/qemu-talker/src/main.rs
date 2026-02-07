//! Simple ESP32-C3 QEMU Talker using nano-ros-bsp-esp32-qemu
//!
//! This example uses the OpenETH NIC in QEMU (no WiFi needed), establishes
//! a zenoh session via static IP, and publishes messages.
//!
//! # Building
//!
//! ```bash
//! just build-examples-esp32-qemu
//! ```
//!
//! # Running (requires QEMU with Espressif fork)
//!
//! ```bash
//! ./scripts/esp32/launch-esp32c3.sh --tap tap-qemu0 \
//!     --binary build/esp32-qemu/esp32-qemu-talker.bin
//! ```

#![no_std]
#![no_main]

use esp_backtrace as _;
use nano_ros_bsp_esp32_qemu::esp_println;
use nano_ros_bsp_esp32_qemu::prelude::*;

/// Topic to publish on
const TOPIC: &[u8] = b"demo/esp32\0";

nano_ros_bsp_esp32_qemu::esp_bootloader_esp_idf::esp_app_desc!();

#[entry]
fn main() -> ! {
    run_node(Config::default(), |node| {
        // Declare publisher
        esp_println::println!("Declaring publisher on topic: demo/esp32");
        let publisher = node.create_publisher(TOPIC)?;
        esp_println::println!("Publisher declared (handle: {})", publisher.handle());

        // Publish messages
        esp_println::println!("");
        esp_println::println!("Publishing messages...");

        let mut msg_buf = [0u8; 64];

        for i in 0..5 {
            // Poll to process network events
            for _ in 0..3 {
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
        esp_println::println!("Done publishing 5 messages.");

        Ok(())
    })
}

/// Format a message into the buffer
fn format_message(buf: &mut [u8], num: u32) -> &[u8] {
    let prefix = b"Hello from QEMU ESP32 #";
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
