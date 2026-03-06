//! Native RTIC-pattern Service Client
//!
//! Validates the RTIC service client pattern on native x86:
//! - `Executor<_, 0, 0>` (zero callback arena)
//! - `spin_once(0)` (non-blocking I/O drive)
//! - `client.call()` + `promise.try_recv()` loop (manual polling)
//!
//! Note: `Promise::wait()` is NOT usable in RTIC because it requires `&mut Executor`,
//! which is `#[local]` to the `net_poll` task. Use `try_recv()` loop instead.
//!
//! This is the native equivalent of `examples/stm32f4/rust/zenoh/rtic-service-client/`.

use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest};
use log::info;
use nros::prelude::*;

fn main() {
    env_logger::init();

    info!("nros RTIC-pattern Service Client (native)");

    let config = ExecutorConfig::from_env().node_name("add_client");
    let mut executor = Executor::<_, 0, 0>::open(&config).expect("Failed to open session");

    let mut node = executor
        .create_node("add_client")
        .expect("Failed to create node");
    let mut client = node
        .create_client::<AddTwoInts>("/add_two_ints")
        .expect("Failed to create client");

    info!("Service client created for /add_two_ints (RTIC pattern)");

    // Stabilization delay
    for _ in 0..200 {
        executor.spin_once(0);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    let test_cases: [(i64, i64); 4] = [(5, 3), (10, 20), (100, 200), (-5, 10)];
    let mut success_count = 0u32;

    for (a, b) in test_cases {
        let request = AddTwoIntsRequest { a, b };
        info!("Calling: {} + {} = ?", a, b);

        let mut promise = match client.call(&request) {
            Ok(p) => p,
            Err(e) => {
                log::error!("Failed to send request: {:?}", e);
                continue;
            }
        };

        // Poll for reply with timeout (~5 seconds) — RTIC-compatible pattern
        let mut got_reply = false;
        for _ in 0..500 {
            executor.spin_once(0);

            if let Ok(Some(reply)) = promise.try_recv() {
                info!("Reply: {} + {} = {}", a, b, reply.sum);
                success_count += 1;
                got_reply = true;
                break;
            }

            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        if !got_reply {
            log::error!("Timeout waiting for reply to {} + {}", a, b);
        }

        // Brief pause between calls
        for _ in 0..50 {
            executor.spin_once(0);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    info!(
        "Done. {} of {} calls succeeded",
        success_count,
        test_cases.len()
    );
}
