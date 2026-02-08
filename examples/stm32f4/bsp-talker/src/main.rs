//! nano-ros STM32F4 Talker Example using BSP
//!
//! Publishes typed `std_msgs/Int32` messages on `/chatter`,
//! compatible with ROS 2 nodes via rmw_zenoh.
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

mod msg {
    use nano_ros_bsp_stm32f4::{Deserialize, RosMessage, Serialize, nano_ros_core};

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

/// Poll interval in milliseconds
const POLL_INTERVAL_MS: u32 = 10;

/// Publish interval in milliseconds
const PUBLISH_INTERVAL_MS: u32 = 1000;

#[entry]
fn main() -> ! {
    run_node(Config::nucleo_f429zi(), |node| {
        info!("Creating publisher for /chatter (std_msgs/Int32)...");
        let publisher = node.create_publisher::<Int32>("/chatter")?;

        info!("Starting publish loop (1 Hz)...");
        let mut counter: i32 = 0;
        let mut last_publish_ms: u64 = 0;

        loop {
            node.spin_once(POLL_INTERVAL_MS);

            let now_ms = node.now_ms();
            if now_ms - last_publish_ms >= PUBLISH_INTERVAL_MS as u64 {
                last_publish_ms = now_ms;
                counter = counter.wrapping_add(1);

                match publisher.publish(&Int32 { data: counter }) {
                    Ok(()) => info!("Published: {}", counter),
                    Err(e) => warn!("Publish failed: {:?}", e),
                }
            }
        }
    })
}
