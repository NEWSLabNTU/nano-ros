//! Mixed-Priority RTIC Talker Example for nros on QEMU MPS2-AN385
//!
//! Publishes `std_msgs/Int32` messages on `/chatter` using RTIC v2's
//! hardware-scheduled async tasks with LAN9118 Ethernet networking.
//!
//! This example demonstrates `ffi-sync` with mixed RTIC priorities:
//! - `net_poll` runs at priority 1 (low) — drives transport I/O
//! - `publish` runs at priority 2 (high) — can preempt `net_poll`
//!
//! Without `ffi-sync`, the higher-priority `publish` task could preempt
//! `net_poll` mid-`spin_once(0)`, corrupting zenoh-pico's global state.
//! With `ffi-sync`, `critical_section::with()` masks interrupts during
//! FFI calls, so the preempting task waits until the section exits.
//!
//! # Running
//!
//! ```bash
//! cargo nano-ros generate
//! cargo run --release
//! ```

#![no_std]
#![no_main]

use panic_semihosting as _;

use nros::prelude::*;
use nros_mps2_an385::{Config, println};
use std_msgs::msg::Int32;

use rtic_monotonics::systick::prelude::*;

systick_monotonic!(Mono, 1000);

// Type aliases for RTIC Local struct annotations
type NrosExecutor = Executor;
type NrosPublisher = EmbeddedPublisher<Int32>;

#[rtic::app(device = mps2_an385_pac, dispatchers = [UARTRX0, UARTTX0])]
mod app {
    use super::*;

    #[shared]
    struct Shared {}

    #[local]
    struct Local {
        executor: NrosExecutor,
        publisher: NrosPublisher,
    }

    #[init]
    fn init(cx: init::Context) -> (Shared, Local) {
        let config = Config::default();
        nros_mps2_an385::init_hardware(&config);

        Mono::start(cx.core.SYST, 25_000_000);

        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("talker");
        let mut executor = Executor::open(&exec_config).unwrap();
        let mut node = executor.create_node("talker").unwrap();
        let publisher = node.create_publisher::<Int32>("/chatter").unwrap();

        net_poll::spawn().unwrap();
        publish::spawn().unwrap();

        (
            Shared {},
            Local {
                executor,
                publisher,
            },
        )
    }

    /// Drive transport I/O — equivalent to rclcpp spin_some().
    ///
    /// Each `spin_once(0)` call processes one round of network I/O.
    /// The 10 ms RTIC yield lets QEMU's I/O loop service the TAP device
    /// (host → LAN9118 RX FIFO path only runs during WFI).
    ///
    /// Runs at priority 1 (low) — can be preempted by `publish` (priority 2).
    #[task(local = [executor], priority = 1)]
    async fn net_poll(cx: net_poll::Context) {
        loop {
            cx.local.executor.spin_once(0);
            Mono::delay(10.millis()).await;
        }
    }

    /// Publish 10 messages at ~1 Hz, then exit.
    ///
    /// Runs at priority 2 (high) — can preempt `net_poll`. The `ffi-sync`
    /// feature ensures `publish()` waits for any in-progress FFI critical
    /// section in `net_poll` before accessing zenoh-pico state.
    #[task(local = [publisher], priority = 2)]
    async fn publish(cx: publish::Context) {
        // Wait for zenoh session establishment
        Mono::delay(2000.millis()).await;

        println!("Publishing messages...");

        for i in 0..10i32 {
            // Poll between publishes for network events
            Mono::delay(1000.millis()).await;

            match cx.local.publisher.publish(&Int32 { data: i }) {
                Ok(()) => println!("Published: {}", i),
                Err(e) => println!("Publish failed: {:?}", e),
            }
        }

        // Drain delay: allow last message to propagate through zenohd
        Mono::delay(2000.millis()).await;

        println!("");
        println!("Done publishing 10 messages.");
        nros_mps2_an385::exit_success();
    }
}
