//! NuttX QEMU ARM Service Client Example
//!
//! Calls the AddTwoInts service on `/add_two_ints`.
//! Uses NuttX QEMU ARM virt (Cortex-A7 + virtio-net).

use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest};
use nros::prelude::*;
use nros_nuttx_qemu_arm::{Config, run};

fn main() {
    run(Config::client(), |config| {
        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("add_two_ints_client");
        let mut executor: Executor<_> = Executor::open(&exec_config)?;
        let mut node = executor.create_node("add_two_ints_client")?;

        println!("Creating service client: /add_two_ints (AddTwoInts)");
        let mut client = node.create_client::<AddTwoInts>("/add_two_ints")?;
        println!("Client ready");
        println!();

        let test_cases = [(5, 3), (10, 20), (100, 200), (-5, 10)];

        for (a, b) in test_cases {
            let request = AddTwoIntsRequest { a, b };
            println!("Calling: {} + {} = ?", a, b);

            let mut promise = client.call(&request)?;
            let response = promise.wait(&mut executor, 5000)?;

            println!("Response: {} + {} = {}", a, b, response.sum);
        }

        println!();
        println!("All service calls completed.");
        Ok::<(), NodeError>(())
    })
}
