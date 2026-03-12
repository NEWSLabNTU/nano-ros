//! RTIC Talker Example for nros on STM32F4
//!
//! Publishes `std_msgs/Int32` messages on `/chatter` using RTIC v2's
//! hardware-scheduled async tasks. Demonstrates the nano-ros + RTIC
//! integration pattern:
//!
//! - `#[init]` calls `board::init_hardware()` and creates nano-ros handles
//! - `net_poll` task drives transport I/O via `spin_once(0)`
//! - `publish` task publishes messages independently (no executor needed)
//! - All nano-ros handles are `#[local]` — no locks required
//!
//! # Hardware
//!
//! - Board: NUCLEO-F429ZI (or similar STM32F4 with Ethernet)
//! - Connect Ethernet cable to the board's RJ45 port
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

defmt::timestamp!("{=u64:us}", { 0 });

use nros::prelude::*;
use nros_stm32f4::Config;
use std_msgs::msg::Int32;

use rtic_monotonics::systick::prelude::*;

systick_monotonic!(Mono, 1000);

// Type aliases for RTIC Local struct annotations
type NrosExecutor = Executor;
type NrosPublisher = EmbeddedPublisher<Int32>;

#[rtic::app(device = stm32f4xx_hal::pac, dispatchers = [USART1, USART2])]
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
        let config = Config::nucleo_f429zi();
        let syst = nros_stm32f4::init_hardware(&config, cx.device, cx.core);

        Mono::start(syst, 168_000_000);

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
    #[task(local = [executor], priority = 1)]
    async fn net_poll(cx: net_poll::Context) {
        loop {
            cx.local.executor.spin_once(0);
            Mono::delay(10.millis()).await;
        }
    }

    /// Publish messages at 1 Hz. Does not require the executor.
    #[task(local = [publisher], priority = 1)]
    async fn publish(cx: publish::Context) {
        // Wait for zenoh session establishment
        Mono::delay(2000.millis()).await;

        defmt::info!("Starting publish loop (1 Hz)...");

        let mut counter: i32 = 0;
        loop {
            counter = counter.wrapping_add(1);

            match cx.local.publisher.publish(&Int32 { data: counter }) {
                Ok(()) => defmt::info!("Published: {}", counter),
                Err(e) => defmt::warn!("Publish failed: {:?}", e),
            }

            Mono::delay(1000.millis()).await;
        }
    }
}
