//! Simple ESP32-C3 QEMU Listener using nano-ros-platform-esp32-qemu
//!
//! Subscribes to typed `std_msgs/Int32` messages on `/chatter`.
//! Compare with qemu-bsp-listener — this is the ESP32-C3 equivalent.
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
//! ./scripts/esp32/launch-esp32c3.sh --tap tap-qemu1 \
//!     --binary build/esp32-qemu/esp32-qemu-listener.bin
//! ```

#![no_std]
#![no_main]

use esp_backtrace as _;
use nano_ros_platform_esp32_qemu::esp_println;
use nano_ros_platform_esp32_qemu::portable_atomic::{AtomicI32, AtomicU32, Ordering};
use nano_ros_platform_esp32_qemu::prelude::*;

mod msg {
    use nano_ros_platform_esp32_qemu::{Deserialize, RosMessage, Serialize, nros_core};

    pub struct Int32 {
        pub data: i32,
    }

    impl Serialize for Int32 {
        fn serialize(
            &self,
            w: &mut nros_core::CdrWriter,
        ) -> core::result::Result<(), nros_core::SerError> {
            w.write_i32(self.data)
        }
    }

    impl Deserialize for Int32 {
        fn deserialize(
            r: &mut nros_core::CdrReader,
        ) -> core::result::Result<Self, nros_core::DeserError> {
            Ok(Self {
                data: r.read_i32()?,
            })
        }
    }

    impl RosMessage for Int32 {
        const TYPE_NAME: &'static str = "std_msgs::msg::dds_::Int32_";
        const TYPE_HASH: &'static str =
            "RIHS01_0000000000000000000000000000000000000000000000000000000000000000";
    }
}

use msg::Int32;

/// Last received Int32 value (portable-atomic provides safe atomics on riscv32imc)
static LAST_VALUE: AtomicI32 = AtomicI32::new(0);

/// Message count
static MSG_COUNT: AtomicU32 = AtomicU32::new(0);

/// Typed subscriber callback
fn on_message(msg: &Int32) {
    LAST_VALUE.store(msg.data, Ordering::Relaxed);
    MSG_COUNT.fetch_add(1, Ordering::Relaxed);
}

nano_ros_platform_esp32_qemu::esp_bootloader_esp_idf::esp_app_desc!();

#[entry]
fn main() -> ! {
    run_node(Config::listener(), |node| {
        esp_println::println!("Subscribing to /chatter (std_msgs/Int32)");

        let _subscription = node.create_subscription::<Int32>("/chatter", on_message)?;

        esp_println::println!("Subscriber declared");
        esp_println::println!("");
        esp_println::println!("Waiting for messages...");

        let mut last_count = 0u32;
        let mut poll_count = 0u32;

        loop {
            // Poll to process network events
            node.spin_once(10);

            // Check for new messages
            let current_count = MSG_COUNT.load(Ordering::Relaxed);
            if current_count > last_count {
                let value = LAST_VALUE.load(Ordering::Relaxed);
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

            // Safety timeout
            if poll_count > 1_000_000 {
                esp_println::println!("");
                esp_println::println!("Timeout waiting for messages.");
                break;
            }
        }

        Ok(())
    })
}
