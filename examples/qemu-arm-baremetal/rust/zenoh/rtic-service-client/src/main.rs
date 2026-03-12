//! RTIC Service Client Example for nros on QEMU MPS2-AN385
//!
//! Calls `AddTwoInts` service using RTIC v2's hardware-scheduled async tasks
//! with LAN9118 Ethernet networking.
//!
//! Uses a single task that owns both the executor and client, calling
//! `spin_once()` between `try_recv()` polls for I/O processing.
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

use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest};
use nros::prelude::*;
use nros_mps2_an385::{Config, println};

use rtic_monotonics::systick::prelude::*;

systick_monotonic!(Mono, 1000);

// Type aliases for RTIC Local struct annotations
type NrosExecutor = Executor;
type NrosServiceClient = nros::EmbeddedServiceClient<AddTwoInts>;

#[rtic::app(device = mps2_an385_pac, dispatchers = [UARTRX0])]
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
        let config = Config::listener();
        nros_mps2_an385::init_hardware(&config);

        Mono::start(cx.core.SYST, 25_000_000);

        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("add_client");
        let mut executor = Executor::open(&exec_config).unwrap();
        let mut node = executor.create_node("add_client").unwrap();
        let client = node.create_client::<AddTwoInts>("/add_two_ints").unwrap();

        call_service::spawn().unwrap();

        (Shared {}, Local { executor, client })
    }

    /// Drive transport I/O and call service in a single task.
    ///
    /// Each `spin_once(0)` call processes one round of network I/O.
    /// The 10 ms RTIC yield lets QEMU's I/O loop service the TAP device
    /// (host → LAN9118 RX FIFO path only runs during WFI).
    #[task(local = [executor, client], priority = 1)]
    async fn call_service(cx: call_service::Context) {
        // Wait for zenoh session + server queryable discovery.
        for _ in 0..500 {
            cx.local.executor.spin_once(0);
            Mono::delay(10.millis()).await;
        }

        println!("Client ready, starting service calls...");

        let test_cases: [(i64, i64); 4] = [(5, 3), (10, 20), (100, 200), (-5, 10)];

        for (a, b) in test_cases {
            let request = AddTwoIntsRequest { a, b };
            println!("Calling: {} + {} = ?", a, b);

            let mut promise = cx.local.client.call(&request).unwrap();

            let mut got_reply = false;
            for _i in 0..3000u32 {
                cx.local.executor.spin_once(0);
                Mono::delay(10.millis()).await;

                match promise.try_recv() {
                    Ok(Some(reply)) => {
                        println!("Reply: {} + {} = {}", a, b, reply.sum);
                        got_reply = true;
                        break;
                    }
                    Ok(None) => {}
                    Err(e) => {
                        println!("try_recv error: {:?}", e);
                        nros_mps2_an385::exit_failure();
                    }
                }
            }
            if !got_reply {
                println!("Service call timed out after 30s");
                nros_mps2_an385::exit_failure();
            }

            cx.local.executor.spin_once(0);
            Mono::delay(10.millis()).await;
        }

        println!("");
        println!("All service calls completed");
        nros_mps2_an385::exit_success();
    }
}
