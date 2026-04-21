//! RTIC Service Server Example for nros on QEMU MPS2-AN385
//!
//! Handles `AddTwoInts` service requests using RTIC v2's hardware-scheduled
//! async tasks with LAN9118 Ethernet networking.
//!
//! - `#[init]` calls `board::init_hardware()` and creates nano-ros handles
//! - `net_poll` task drives transport I/O via `spin_once(0)`
//! - `serve` task polls for requests via `handle_request()`
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

use example_interfaces::srv::{AddTwoInts, AddTwoIntsResponse};
use nros::prelude::*;
use nros_mps2_an385::{Config, println};

use rtic_monotonics::systick::prelude::*;

systick_monotonic!(Mono, 1000);

// Type aliases for RTIC Local struct annotations
type NrosExecutor = Executor;
type NrosServiceServer = nros::EmbeddedServiceServer<AddTwoInts>;

#[rtic::app(device = mps2_an385_pac, dispatchers = [UARTRX0, UARTTX0])]
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
        let config = Config::from_toml(include_str!("../config.toml"));
        nros_mps2_an385::init_hardware(&config);

        Mono::start(cx.core.SYST, 25_000_000);

        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("add_server");
        let mut executor = Executor::open(&exec_config).unwrap();
        let mut node = executor.create_node("add_server").unwrap();
        let service = node.create_service::<AddTwoInts>("/add_two_ints").unwrap();

        net_poll::spawn().unwrap();
        serve::spawn().unwrap();

        (Shared {}, Local { executor, service })
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

    /// Poll for and handle service requests.
    #[task(local = [service], priority = 1)]
    async fn serve(cx: serve::Context) {
        // Wait for zenoh session establishment
        Mono::delay(2000.millis()).await;

        println!("Service server ready: /add_two_ints");

        loop {
            match cx.local.service.handle_request(|req| {
                let sum = req.a + req.b;
                println!("Handled: {} + {} = {}", req.a, req.b, sum);
                AddTwoIntsResponse { sum }
            }) {
                Ok(true) => {}  // handled a request
                Ok(false) => {} // no request available
                Err(e) => println!("Service error: {:?}", e),
            }

            Mono::delay(10.millis()).await;
        }
    }
}
