//! Phase 141.D — wake-latency P99 microbench, PUBLISHER image (issue #0317).
//!
//! Publishes `std_msgs/Int32` on [`wake_latency_cortex_m3::TOPIC`] at 100 Hz
//! (the burst scenario emits [`BURST`](wake_latency_cortex_m3::BURST) messages
//! back-to-back per tick) so the SEPARATE subscriber image
//! (`src/main.rs`) gets a real transport-arrival wake per message through the
//! host `zenohd` router. Runs until the test kills its QEMU.
//!
//! A distinct IP/MAC ([`publisher_config`]) gives this image a distinct zenoh
//! session id from the subscriber, so the router routes between two peers rather
//! than one session seeing itself.

#![no_std]
#![no_main]

use nros::prelude::*;
use nros_board_mps2_an385_freertos::{Mps2An385, println};
use panic_semihosting as _;
use std_msgs::msg::Int32;
use wake_latency_cortex_m3::{BURST, TOPIC, publisher_config};

// The C startup jumps to the Rust `main` symbol (see #0273).
#[unsafe(no_mangle)]
extern "C" fn main() -> ! {
    let _ = Mps2An385::run_bare(publisher_config(), |config| {
        let exec_config = ExecutorConfig::new(config.base.zenoh_locator)
            .domain_id(config.base.domain_id)
            .node_name("wake-latency-pub");
        nros_rmw_zenoh::register().expect("Failed to register RMW backend");
        let mut executor = Executor::open(&exec_config)?;
        let publisher = {
            let mut node = executor.create_node("wake-latency-pub")?;
            node.create_publisher::<Int32>(TOPIC)?
        };

        // Let discovery + the subscriber's session settle before publishing.
        for _ in 0..50 {
            executor.spin_once(core::time::Duration::from_millis(10));
        }

        println!("wake-latency-pub ready — publishing on {}", TOPIC);

        let mut emitted: i32 = 0;
        executor.register_timer(
            nros::TimerDuration::from_millis(10), // 100 Hz
            move || {
                for _ in 0..BURST {
                    let _ = publisher.publish(&Int32 { data: emitted });
                    emitted = emitted.wrapping_add(1);
                }
            },
        )?;

        // Publish for the firmware's lifetime; the test kills this QEMU once the
        // subscriber image has dumped its histogram.
        loop {
            executor.spin_once(core::time::Duration::from_millis(10));
        }

        #[allow(unreachable_code)]
        Ok::<(), NodeError>(())
    });
    unreachable!()
}
