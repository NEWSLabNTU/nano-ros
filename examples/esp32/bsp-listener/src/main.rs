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

use core::cell::RefCell;
use core::ffi::c_void;
use core::ptr;

use esp_backtrace as _;
use nano_ros_bsp_esp32::critical_section::{self, Mutex};
use nano_ros_bsp_esp32::esp_println;
use nano_ros_bsp_esp32::portable_atomic::{AtomicU32, Ordering};
use nano_ros_bsp_esp32::prelude::*;

/// WiFi credentials (set via environment variables at compile time)
const SSID: &str = env!("SSID");
const PASSWORD: &str = env!("PASSWORD");

/// Topic to subscribe to
const TOPIC: &[u8] = b"demo/esp32\0";

/// Message buffer for storing received messages
const MSG_BUFFER_SIZE: usize = 256;

/// Received message state, protected by critical section for safe access
struct MsgBuffer {
    data: [u8; MSG_BUFFER_SIZE],
    len: usize,
}

static MSG_BUFFER: Mutex<RefCell<MsgBuffer>> = Mutex::new(RefCell::new(MsgBuffer {
    data: [0u8; MSG_BUFFER_SIZE],
    len: 0,
}));

/// Message count (portable-atomic provides safe atomics on riscv32imc)
static MSG_COUNT: AtomicU32 = AtomicU32::new(0);

/// Subscriber callback - called when a message is received
extern "C" fn on_message(data: *const u8, len: usize, _ctx: *mut c_void) {
    critical_section::with(|cs| {
        let mut buf = MSG_BUFFER.borrow_ref_mut(cs);
        let copy_len = len.min(MSG_BUFFER_SIZE);
        // SAFETY: `data` is a valid pointer to `len` bytes provided by zenoh-pico C callback
        unsafe {
            ptr::copy_nonoverlapping(data, buf.data.as_mut_ptr(), copy_len);
        }
        buf.len = copy_len;
    });
    MSG_COUNT.fetch_add(1, Ordering::Relaxed);
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
                let current_count = MSG_COUNT.load(Ordering::Relaxed);
                if current_count > last_count {
                    // New message received — read buffer under critical section
                    critical_section::with(|cs| {
                        let buf = MSG_BUFFER.borrow_ref(cs);
                        let msg = &buf.data[..buf.len];
                        if let Ok(s) = core::str::from_utf8(msg) {
                            esp_println::println!("Received [{}]: {}", current_count, s);
                        } else {
                            esp_println::println!(
                                "Received [{}]: <{} bytes binary>",
                                current_count,
                                buf.len
                            );
                        }
                    });
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
