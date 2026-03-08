//! ThreadX QEMU RISC-V Action Client
//!
//! Sends a `example_interfaces/Fibonacci` goal to `/fibonacci`.

#![no_std]
#![no_main]

use example_interfaces::action::{Fibonacci, FibonacciGoal};
use nros::prelude::*;
use nros_threadx_qemu_riscv64::{Config, println, run};

#[unsafe(no_mangle)]
extern "C" fn main() -> ! {
    run(Config::listener(), |config| {
        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("fibonacci_action_client");
        let mut executor = Executor::<_, 8, 8192>::open(&exec_config)?;
        let mut node = executor.create_node("fibonacci_action_client")?;

        let mut client = node.create_action_client::<Fibonacci>("/fibonacci")?;
        println!("Action client ready for /fibonacci");

        // Wait for server to be available
        for _ in 0..500 {
            executor.spin_once(10);
        }

        let goal = FibonacciGoal { order: 5 };
        println!("Sending goal: order={}", goal.order);

        let (goal_id, mut promise) = client.send_goal(&goal)?;

        // Poll for goal acceptance
        let mut accepted = false;
        for _ in 0..5000 {
            executor.spin_once(10);
            if let Some(result) = promise.try_recv()? {
                accepted = result;
                break;
            }
        }

        if !accepted {
            println!("Goal was rejected or timed out");
            return Ok(());
        }
        println!("Goal accepted: {:?}", goal_id);

        // Poll for result
        println!("Requesting result...");
        let mut result_promise = client.get_result(&goal_id)?;
        for _ in 0..10000u32 {
            executor.spin_once(10);
            if let Some((status, result)) = result_promise.try_recv()? {
                println!("Result status: {:?}", status);
                println!("Fibonacci sequence: {:?}", result.sequence);
                println!("Action completed successfully.");
                return Ok(());
            }
        }

        println!("Timeout waiting for result.");
        Ok::<(), NodeError>(())
    })
}
