//! QEMU Talker - ROS 2 Int32 Publisher for MPS2-AN385
//!
//! Publishes typed `std_msgs/Int32` messages on `/chatter`,
//! compatible with ROS 2 nodes via rmw_zenoh.

#![no_std]
#![no_main]

use nano_ros_bsp_qemu::prelude::*;
use nano_ros_bsp_qemu::println;
use panic_semihosting as _;

mod msg {
    use nano_ros_bsp_qemu::{Deserialize, RosMessage, Serialize, nano_ros_core};

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

#[entry]
fn main() -> ! {
    run_node(Config::default(), |node| {
        println!("Declaring publisher on /chatter (std_msgs/Int32)");
        let publisher = node.create_publisher::<Int32>("/chatter")?;
        println!("Publisher declared");

        println!("");
        println!("Publishing messages...");

        for i in 0..10i32 {
            // Poll to process network events
            for _ in 0..100 {
                node.spin_once(10);
            }

            if let Err(e) = publisher.publish(&Int32 { data: i }) {
                println!("Publish failed: {:?}", e);
            } else {
                println!("Published: {}", i);
            }
        }

        println!("");
        println!("Done publishing 10 messages.");

        Ok(())
    })
}
