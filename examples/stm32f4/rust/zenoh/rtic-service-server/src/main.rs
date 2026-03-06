//! RTIC Service Server Example for nros on STM32F4
//!
//! Handles `AddTwoInts` service requests using RTIC v2's hardware-scheduled
//! async tasks. Demonstrates the RTIC service pattern:
//!
//! - `#[init]` calls `board::init_hardware()` and creates nano-ros handles
//! - `net_poll` task drives transport I/O via `spin_once(0)`
//! - `serve` task polls for requests via `handle_request()`
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

use example_interfaces::srv::{AddTwoInts, AddTwoIntsResponse};
use nros::prelude::*;
use nros_stm32f4::Config;

use rtic_monotonics::systick::prelude::*;

systick_monotonic!(Mono, 1000);

// Type aliases for RTIC Local struct annotations
type RmwSrvServer = nros::internals::RmwServiceServer;
type NrosExecutor = Executor<nros::internals::RmwSession, 0, 0>;
type NrosServiceServer = nros::EmbeddedServiceServer<AddTwoInts, RmwSrvServer>;

#[rtic::app(device = stm32f4xx_hal::pac, dispatchers = [USART1, USART2])]
mod app {
    use super::*;

    #[shared]
    struct Shared {}

    #[local]
    struct Local {
        executor: NrosExecutor,
        service: NrosServiceServer,
    }

    #[init]
    fn init(cx: init::Context) -> (Shared, Local) {
        let config = Config::nucleo_f429zi();
        let syst = nros_stm32f4::init_hardware(&config, cx.device, cx.core);

        Mono::start(syst, 168_000_000);

        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("add_server");
        let mut executor = Executor::<_, 0, 0>::open(&exec_config).unwrap();
        let mut node = executor.create_node("add_server").unwrap();
        let service = node.create_service::<AddTwoInts>("/add_two_ints").unwrap();

        net_poll::spawn().unwrap();
        serve::spawn().unwrap();

        (Shared {}, Local { executor, service })
    }

    /// Drive transport I/O — equivalent to rclcpp spin_some().
    #[task(local = [executor], priority = 1)]
    async fn net_poll(cx: net_poll::Context) {
        loop {
            cx.local.executor.spin_once(0);
            Mono::delay(10.millis()).await;
        }
    }

    /// Poll for and handle service requests.
    #[task(local = [service], priority = 1)]
    async fn serve(cx: serve::Context) {
        // Wait for zenoh session establishment
        Mono::delay(2000.millis()).await;

        defmt::info!("Service server ready: /add_two_ints");

        loop {
            match cx.local.service.handle_request(|req| {
                let sum = req.a + req.b;
                defmt::info!("Request: {} + {} = {}", req.a, req.b, sum);
                AddTwoIntsResponse { sum }
            }) {
                Ok(true) => {}  // handled a request
                Ok(false) => {} // no request available
                Err(e) => defmt::warn!("Service error: {:?}", e),
            }

            Mono::delay(10.millis()).await;
        }
    }
}
