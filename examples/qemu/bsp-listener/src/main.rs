//! Simple QEMU Listener using nano-ros-bsp-qemu
//!
//! Subscribes to typed `std_msgs/Int32` messages on `/chatter`.
//! Compare with qemu-rs-listener to see the reduced boilerplate.

#![no_std]
#![no_main]

use core::sync::atomic::{AtomicU32, Ordering};

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

/// Last received Int32 value
static mut LAST_VALUE: i32 = 0;

/// Message count (atomic for safe callback access)
static MSG_COUNT: AtomicU32 = AtomicU32::new(0);

/// Typed subscriber callback
fn on_message(msg: &Int32) {
    unsafe {
        LAST_VALUE = msg.data;
    }
    MSG_COUNT.fetch_add(1, Ordering::SeqCst);
}

#[entry]
fn main() -> ! {
    // Use listener config (different IP/MAC than talker)
    run_node(Config::listener(), |node| {
        println!("Subscribing to /chatter (std_msgs/Int32)");

        let _subscription = node.create_subscription::<Int32>("/chatter", on_message)?;

        println!("Subscriber declared");
        println!("");
        println!("Waiting for messages...");

        let mut last_count = 0u32;
        let mut poll_count = 0u32;

        loop {
            node.spin_once(10);

            let current_count = MSG_COUNT.load(Ordering::SeqCst);
            if current_count > last_count {
                let value = unsafe { LAST_VALUE };
                println!("Received [{}]: {}", current_count, value);
                last_count = current_count;

                if current_count >= 10 {
                    println!("");
                    println!("Received 10 messages.");
                    break;
                }
            }

            poll_count += 1;
            if poll_count > 100000 {
                println!("");
                println!("Timeout waiting for messages.");
                break;
            }
        }

        Ok(())
    })
}
