//! Simple ESP32-C3 QEMU Talker using nano-ros-bsp-esp32-qemu
//!
//! Publishes typed `std_msgs/Int32` messages on `/chatter`.
//! Compare with qemu-bsp-talker — this is the ESP32-C3 equivalent.
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

mod msg {
    use nano_ros_bsp_esp32_qemu::{Deserialize, RosMessage, Serialize, nano_ros_core};

    pub struct Int32 {
        pub data: i32,
    }

    impl Serialize for Int32 {
        fn serialize(
            &self,
            w: &mut nano_ros_core::CdrWriter,
        ) -> core::result::Result<(), nano_ros_core::SerError> {
            w.write_i32(self.data)
        }
    }

    impl Deserialize for Int32 {
        fn deserialize(
            r: &mut nano_ros_core::CdrReader,
        ) -> core::result::Result<Self, nano_ros_core::DeserError> {
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

nano_ros_bsp_esp32_qemu::esp_bootloader_esp_idf::esp_app_desc!();

#[entry]
fn main() -> ! {
    run_node(Config::default(), |node| {
        esp_println::println!("Declaring publisher on /chatter (std_msgs/Int32)");
        let publisher = node.create_publisher::<Int32>("/chatter")?;
        esp_println::println!("Publisher declared");

        esp_println::println!("");
        esp_println::println!("Publishing messages...");

        for i in 0..5i32 {
            // Poll to process network events
            for _ in 0..3 {
                node.spin_once(10);
            }

            if let Err(e) = publisher.publish(&Int32 { data: i }) {
                esp_println::println!("Publish failed: {:?}", e);
            } else {
                esp_println::println!("Published: {}", i);
            }
        }

        esp_println::println!("");
        esp_println::println!("Done publishing 5 messages.");

        Ok(())
    })
}
