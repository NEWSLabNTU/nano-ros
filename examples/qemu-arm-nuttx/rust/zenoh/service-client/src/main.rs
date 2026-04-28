//! NuttX QEMU ARM Service Client Example
//!
//! Calls the AddTwoInts service on `/add_two_ints`.
//! Uses NuttX QEMU ARM virt (Cortex-A7 + virtio-net).

use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest};
use nros::prelude::*;
use nros_board_nuttx_qemu_arm::{Config, run};

fn main() {
    run(Config::from_toml(include_str!("../config.toml")), |config| {
        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("add_two_ints_client");
        let mut executor: Executor = Executor::open(&exec_config)?;
        let mut node = executor.create_node("add_two_ints_client")?;

        println!("Creating service client: /add_two_ints (AddTwoInts)");
        let mut client = node.create_client::<AddTwoInts>("/add_two_ints")?;
        println!("Client created — waiting for server discovery...");

        // Race-3 fix: gate the first `call()` on liveliness-token discovery.
        // On a multi-threaded zenoh-pico backend (NuttX), the client and
        // server boot in parallel and the first request can otherwise race
        // the router-side propagation of the server's queryable.
        // `wait_for_service` issues a `z_liveliness_get` and lets the
        // executor cooperatively spin until either a matching token reports
        // back or the timeout expires.
        let server_seen = client.wait_for_service(
            &mut executor,
            core::time::Duration::from_secs(10),
        )?;
        if !server_seen {
            eprintln!("Service /add_two_ints not visible after 10s — bailing");
            return Err(NodeError::Timeout);
        }
        println!("Server discovered — sending requests");
        println!();

        let test_cases = [(5, 3), (10, 20), (100, 200), (-5, 10)];

        for (a, b) in test_cases {
            let request = AddTwoIntsRequest { a, b };
            println!("Calling: {} + {} = ?", a, b);

            let mut promise = client.call(&request)?;
            let response = promise.wait(&mut executor, core::time::Duration::from_millis(5000))?;

            println!("Response: {} + {} = {}", a, b, response.sum);
        }

        println!();
        println!("All service calls completed.");
        Ok::<(), NodeError>(())
    })
}
