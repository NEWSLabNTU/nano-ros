//! RTIC Listener Example for nros on QEMU MPS2-AN385
//!
//! Subscribes to `std_msgs/Int32` on `/chatter` using RTIC v2's
//! hardware-scheduled async tasks with LAN9118 Ethernet networking.
//!
//! - `#[init]` calls `board::init_hardware()` and creates nano-ros handles
//! - `net_poll` task drives transport I/O via `spin_once(0)`
//! - `listen` task polls for messages via `try_recv()`
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
use nros_mps2_an385::{Config, println};
use std_msgs::msg::Int32;

use rtic_monotonics::systick::prelude::*;

systick_monotonic!(Mono, 1000);

// Type aliases for RTIC Local struct annotations
type RmwSub = nros::internals::RmwSubscriber;
type NrosExecutor = Executor<nros::internals::RmwSession, 0, 0>;
type NrosSubscription = Subscription<Int32, RmwSub>;

#[rtic::app(device = mps2_an385_pac, dispatchers = [UARTRX0, UARTTX0])]
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
        let config = Config::listener();
        nros_mps2_an385::init_hardware(&config);

        Mono::start(cx.core.SYST, 25_000_000);

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

    /// Poll for incoming messages. Exits after receiving 10 messages.
    #[task(local = [subscription], priority = 1)]
    async fn listen(cx: listen::Context) {
        println!("Waiting for messages on /chatter...");

        let mut count: u32 = 0;
        let mut timeout: u32 = 0;
        loop {
            if let Some(msg) = cx.local.subscription.try_recv().unwrap() {
                count += 1;
                println!("Received [{}]: {}", count, msg.data);

                if count >= 10 {
                    println!("");
                    println!("Received 10 messages.");
                    nros_mps2_an385::exit_success();
                }
            }

            timeout += 1;
            if timeout > 100_000 {
                println!("");
                println!("Timeout waiting for messages.");
                nros_mps2_an385::exit_failure();
            }

            Mono::delay(1.millis()).await;
        }
    }
}
