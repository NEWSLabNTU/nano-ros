//! ThreadX Linux Action Client
//!
//! Sends a `example_interfaces/Fibonacci` goal to `/fibonacci`.

use example_interfaces::action::{Fibonacci, FibonacciGoal};
use nros::prelude::*;
use nros_threadx_linux::{Config, run};

fn main() {
    run(Config::from_toml(include_str!("../config.toml")), |config| {
        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("fibonacci_action_client");
        let mut executor = Executor::open(&exec_config)?;
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
        for _ in 0..10000 {
            executor.spin_once(10);
            if let Some((status, result)) = result_promise.try_recv()? {
                println!("Result status: {:?}", status);
                println!("Fibonacci sequence: {:?}", result.sequence);
                println!();
                println!("Action completed successfully.");
                return Ok(());
            }
        }

        println!("Timeout waiting for result.");
        Ok::<(), NodeError>(())
    })
}
