//! ThreadX Linux Service Client
//!
//! Calls `example_interfaces/AddTwoInts` on `/add_two_ints`.

use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest};
use nros::prelude::*;
use nros_threadx_linux::{Config, run};

fn main() {
    run(Config::from_toml(include_str!("../config.toml")), |config| {
        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("add_two_ints_client");
        let mut executor = Executor::open(&exec_config)?;
        let mut node = executor.create_node("add_two_ints_client")?;

        let mut client = node.create_client::<AddTwoInts>("/add_two_ints")?;
        println!("Service client ready for /add_two_ints");

        // Wait for service to be available
        for _ in 0..500 {
            executor.spin_once(10);
        }

        let test_cases: &[(i64, i64)] = &[(5, 3), (10, 20), (100, 200), (-5, 10)];

        for &(a, b) in test_cases {
            let request = AddTwoIntsRequest { a, b };
            println!("Calling: {} + {} = ?", a, b);

            let mut promise = client.call(&request)?;

            // Poll for response
            let mut response = None;
            for _ in 0..5000 {
                executor.spin_once(10);
                if let Some(reply) = promise.try_recv()? {
                    response = Some(reply);
                    break;
                }
            }

            match response {
                Some(resp) => {
                    println!("Response: {} + {} = {}", a, b, resp.sum);
                    if resp.sum != a + b {
                        println!("ERROR: expected {}", a + b);
                    }
                }
                None => {
                    println!("ERROR: timeout waiting for response");
                }
            }
        }

        println!();
        println!("All service calls completed.");
        Ok::<(), NodeError>(())
    })
}
