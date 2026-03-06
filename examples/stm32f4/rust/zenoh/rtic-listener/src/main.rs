//! RTIC Listener Example for nros on STM32F4
//!
//! Subscribes to `std_msgs/Int32` on `/chatter` using RTIC v2's
//! hardware-scheduled async tasks. Demonstrates the nano-ros + RTIC
//! integration pattern:
//!
//! - `#[init]` calls `board::init_hardware()` and creates nano-ros handles
//! - `net_poll` task drives transport I/O via `spin_once(0)`
//! - `listen` task polls for messages via `try_recv()`
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
type RmwSub = nros::internals::RmwSubscriber;
type NrosExecutor = Executor<nros::internals::RmwSession, 0, 0>;
type NrosSubscription = Subscription<Int32, RmwSub>;

#[rtic::app(device = stm32f4xx_hal::pac, dispatchers = [USART1, USART2])]
mod app {
    use super::*;

    #[shared]
    struct Shared {}

    #[local]
    struct Local {
        executor: NrosExecutor,
        subscription: NrosSubscription,
    }

    #[init]
    fn init(cx: init::Context) -> (Shared, Local) {
        let config = Config::nucleo_f429zi();
        let syst = nros_stm32f4::init_hardware(&config, cx.device, cx.core);

        Mono::start(syst, 168_000_000);

        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("listener");
        let mut executor = Executor::<_, 0, 0>::open(&exec_config).unwrap();
        let mut node = executor.create_node("listener").unwrap();
        let subscription = node.create_subscription::<Int32>("/chatter").unwrap();

        net_poll::spawn().unwrap();
        listen::spawn().unwrap();

        (
            Shared {},
            Local {
                executor,
                subscription,
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

    /// Poll for incoming messages. Does not require the executor.
    #[task(local = [subscription], priority = 1)]
    async fn listen(cx: listen::Context) {
        defmt::info!("Waiting for messages on /chatter...");

        let mut count: u32 = 0;
        loop {
            if let Some(msg) = cx.local.subscription.try_recv().unwrap() {
                count += 1;
                defmt::info!("Received [{}]: {}", count, msg.data);
            }
            Mono::delay(10.millis()).await;
        }
    }
}
