//! Simple ESP32 WiFi Listener using nano-ros-bsp-esp32
//!
//! This example connects to WiFi, establishes a zenoh session, and subscribes
//! to messages. Compare with the QEMU bsp-listener to see the similar API.
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

use core::ffi::c_void;
use core::ptr;

use esp_backtrace as _;
use nano_ros_bsp_esp32::esp_println;
use nano_ros_bsp_esp32::prelude::*;

/// WiFi credentials (set via environment variables at compile time)
const SSID: &str = env!("SSID");
const PASSWORD: &str = env!("PASSWORD");

/// Topic to subscribe to
const TOPIC: &[u8] = b"demo/esp32\0";

/// Message buffer for storing received messages
const MSG_BUFFER_SIZE: usize = 256;
static mut MSG_BUFFER: [u8; MSG_BUFFER_SIZE] = [0u8; MSG_BUFFER_SIZE];
static mut MSG_LEN: usize = 0;

/// Message count (using static mut since ESP32-C3 is single-core
/// and callbacks run in the same polling context)
static mut MSG_COUNT: u32 = 0;

/// Subscriber callback - called when a message is received
#[allow(static_mut_refs)]
extern "C" fn on_message(data: *const u8, len: usize, _ctx: *mut c_void) {
    // Copy message to buffer
    unsafe {
        let copy_len = len.min(MSG_BUFFER_SIZE);
        ptr::copy_nonoverlapping(data, MSG_BUFFER.as_mut_ptr(), copy_len);
        MSG_LEN = copy_len;
    }

    // Increment message count
    unsafe {
        MSG_COUNT += 1;
    }
}

nano_ros_bsp_esp32::esp_bootloader_esp_idf::esp_app_desc!();

#[entry]
fn main() -> ! {
    run_node(
        NodeConfig::new(WifiConfig::new(SSID, PASSWORD)),
        |node| {
            // Declare subscriber
            esp_println::println!("Subscribing to topic: demo/esp32");

            let _subscriber =
                unsafe { node.create_subscriber(TOPIC, Some(on_message), ptr::null_mut()) }?;

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
                let current_count = unsafe { MSG_COUNT };
                if current_count > last_count {
                    // New message received
                    #[allow(static_mut_refs)]
                    unsafe {
                        let msg = &MSG_BUFFER[..MSG_LEN];
                        if let Ok(s) = core::str::from_utf8(msg) {
                            esp_println::println!("Received [{}]: {}", current_count, s);
                        } else {
                            esp_println::println!(
                                "Received [{}]: <{} bytes binary>",
                                current_count,
                                MSG_LEN
                            );
                        }
                    }
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
