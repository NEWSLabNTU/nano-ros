//! RTIC Talker Example for nros on QEMU MPS2-AN385
//!
//! Publishes `std_msgs/Int32` messages on `/chatter` using RTIC v2's
//! hardware-scheduled async tasks with LAN9118 Ethernet networking.
//!
//! - `#[init]` calls `board::init_hardware()` and creates nano-ros handles
//! - `net_poll` task drives transport I/O via `spin_once(0)`
//! - `publish` task publishes 10 messages then exits via semihosting
//! - All nano-ros handles are `#[local]` — no locks required
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
use nros_board_mps2_an385::{Config, println};
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
        let config = Config::from_toml(include_str!("../config.toml"));
        nros_board_mps2_an385::init_hardware(&config);

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
    #[task(local = [executor], priority = 1)]
    async fn net_poll(cx: net_poll::Context) {
        loop {
            cx.local
                .executor
                .spin_once(core::time::Duration::from_millis(0));
            Mono::delay(10.millis()).await;
        }
    }

    /// Publish at ~1 Hz forever.
    #[task(local = [publisher], priority = 1)]
    async fn publish(cx: publish::Context) {
        // Wait for zenoh session establishment
        Mono::delay(2000.millis()).await;

        println!("Publishing messages...");

        let mut count: i32 = 0;
        loop {
            Mono::delay(1000.millis()).await;

            match cx.local.publisher.publish(&Int32 { data: count }) {
                Ok(()) => println!("Published: {}", count),
                Err(e) => println!("Publish failed: {:?}", e),
            }
            count = count.wrapping_add(1);
        }
    }
}
