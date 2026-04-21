//! RTIC Service Client Example for nros on STM32F4
//!
//! Calls `AddTwoInts` service using RTIC v2's hardware-scheduled async tasks.
//! Demonstrates the RTIC service client pattern:
//!
//! - `#[init]` calls `board::init_hardware()` and creates nano-ros handles
//! - `net_poll` task drives transport I/O via `spin_once(0)`
//! - `call_service` task uses `try_recv()` loop to poll for reply
//!   (RTIC cannot use `Promise::wait()` since executor is `#[local]` to net_poll)
//! - All nano-ros handles are `#[local]` — no locks required
//!
//! # Hardware
//!
//! - Board: NUCLEO-F429ZI (or similar STM32F4 with Ethernet)

#![no_std]
#![no_main]

use defmt_rtt as _;
use panic_probe as _;

defmt::timestamp!("{=u64:us}", { 0 });

use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest};
use nros::prelude::*;
use nros_stm32f4::Config;

use rtic_monotonics::systick::prelude::*;

systick_monotonic!(Mono, 1000);

// Type aliases for RTIC Local struct annotations
type NrosExecutor = Executor;
type NrosServiceClient = nros::EmbeddedServiceClient<AddTwoInts>;

#[rtic::app(device = stm32f4xx_hal::pac, dispatchers = [USART1, USART2])]
mod app {
    use super::*;

    #[shared]
    struct Shared {}

    #[local]
    struct Local {
        executor: NrosExecutor,
        client: NrosServiceClient,
    }

    #[init]
    fn init(cx: init::Context) -> (Shared, Local) {
        let config = Config::nucleo_f429zi();
        let syst = nros_stm32f4::init_hardware(&config, cx.device, cx.core);

        Mono::start(syst, 168_000_000);

        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("add_client");
        let mut executor = Executor::open(&exec_config).unwrap();
        let mut node = executor.create_node("add_client").unwrap();
        let client = node.create_client::<AddTwoInts>("/add_two_ints").unwrap();

        net_poll::spawn().unwrap();
        call_service::spawn().unwrap();

        (Shared {}, Local { executor, client })
    }

    /// Drive transport I/O — equivalent to rclcpp spin_some().
    #[task(local = [executor], priority = 1)]
    async fn net_poll(cx: net_poll::Context) {
        loop {
            cx.local
                .executor
                .spin_once(core::time::Duration::from_millis(0));
            Mono::delay(10.millis()).await;
        }
    }

    /// Call the service and poll for the reply using try_recv() loop.
    ///
    /// Note: `Promise::wait()` is NOT usable here because it requires `&mut Executor`,
    /// which is `#[local]` to the `net_poll` task. Instead, use a `try_recv()` +
    /// `Mono::delay().await` loop. The net_poll task drives I/O concurrently.
    #[task(local = [client], priority = 1)]
    async fn call_service(cx: call_service::Context) {
        // Wait for zenoh session and server to be ready
        Mono::delay(3000.millis()).await;

        let test_cases: [(i64, i64); 4] = [(5, 3), (10, 20), (100, 200), (-5, 10)];

        for (a, b) in test_cases {
            let request = AddTwoIntsRequest { a, b };
            defmt::info!("Calling: {} + {} = ?", a, b);

            let mut promise = cx.local.client.call(&request).unwrap();

            // Poll for reply with timeout (~5 seconds)
            let mut timeout = 500u32;
            let reply = loop {
                if let Ok(Some(reply)) = promise.try_recv() {
                    break Some(reply);
                }
                if timeout == 0 {
                    break None;
                }
                timeout -= 1;
                Mono::delay(10.millis()).await;
            };

            match reply {
                Some(r) => defmt::info!("Reply: {} + {} = {}", a, b, r.sum),
                None => defmt::warn!("Timeout waiting for reply"),
            }

            Mono::delay(500.millis()).await;
        }

        defmt::info!("All service calls completed");
    }
}
